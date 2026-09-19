// Explicit acceptance test for a signed, installed TYU Microphone. Does not
// install anything.
#define NOMINMAX
#include <windows.h>
#include <mmdeviceapi.h>
#include "protocol.h"
#include <audioclient.h>
#include <cmath>
#include <cstdio>
#include <functiondiscoverykeys_devpkey.h>
#include <ksmedia.h>
#include <vector>
#include <wrl/client.h>
using Microsoft::WRL::ComPtr;
#define CHECK(expr)                                                                                          \
    do {                                                                                                     \
        HRESULT h = (expr);                                                                                  \
        if (FAILED(h)) {                                                                                     \
            std::printf("FAIL line %d HRESULT 0x%08lx\n", __LINE__, h);                                      \
            return 1;                                                                                        \
        }                                                                                                    \
    } while (0)
struct DeviceHandle {
    HANDLE value = INVALID_HANDLE_VALUE;
    ~DeviceHandle() {
        if (value != INVALID_HANDLE_VALUE) {
            DWORD n = 0;
            DeviceIoControl(value, TYU_MIC_IOCTL_RESET, nullptr, 0, nullptr, 0, &n, nullptr);
            CloseHandle(value);
        }
    }
};
struct ComRuntime {
    HRESULT status = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    ~ComRuntime() {
        if (SUCCEEDED(status))
            CoUninitialize();
    }
};
int main() {
    ComRuntime runtime;
    CHECK(runtime.status);
    DeviceHandle control;
    control.value = CreateFileW(L"\\\\.\\TYUMicrophone", GENERIC_READ | GENERIC_WRITE, 0, nullptr,
                                OPEN_EXISTING, 0, nullptr);
    if (control.value == INVALID_HANDLE_VALUE) {
        std::printf("TYU Microphone driver unavailable: %lu\n", GetLastError());
        return 2;
    }
    TyuMicInfo info{};
    DWORD returned = 0;
    if (!DeviceIoControl(control.value, TYU_MIC_IOCTL_INFO, nullptr, 0, &info, sizeof(info), &returned,
                         nullptr) ||
        returned != sizeof(info) || info.version != TYU_MIC_VERSION || !info.ready) {
        std::puts("TYU driver not ready");
        return 2;
    }
    ComPtr<IMMDeviceEnumerator> enumerator;
    CHECK(CoCreateInstance(__uuidof(MMDeviceEnumerator), nullptr, CLSCTX_ALL, IID_PPV_ARGS(&enumerator)));
    ComPtr<IMMDeviceCollection> devices;
    CHECK(enumerator->EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE, &devices));
    UINT count = 0;
    CHECK(devices->GetCount(&count));
    ComPtr<IMMDevice> microphone;
    for (UINT i = 0; i < count; i++) {
        ComPtr<IMMDevice> device;
        CHECK(devices->Item(i, &device));
        ComPtr<IPropertyStore> props;
        CHECK(device->OpenPropertyStore(STGM_READ, &props));
        PROPVARIANT name{};
        CHECK(props->GetValue(PKEY_Device_FriendlyName, &name));
        bool matches = name.vt == VT_LPWSTR && wcsstr(name.pwszVal, L"TYU Microphone");
        PropVariantClear(&name);
        if (matches) {
            microphone = device;
            break;
        }
    }
    if (!microphone) {
        std::puts("TYU driver opened, but Windows has no active TYU capture endpoint");
        return 3;
    }
    ComPtr<IAudioClient> client;
    CHECK(microphone->Activate(__uuidof(IAudioClient), CLSCTX_ALL, nullptr,
                               reinterpret_cast<void **>(client.GetAddressOf())));
    WAVEFORMATEX format{};
    format.wFormatTag = WAVE_FORMAT_PCM;
    format.nChannels = 1;
    format.nSamplesPerSec = 48000;
    format.wBitsPerSample = 16;
    format.nBlockAlign = 2;
    format.nAvgBytesPerSec = 96000;
    CHECK(client->Initialize(AUDCLNT_SHAREMODE_SHARED,
                             AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                             1000000, 0, &format, nullptr));
    ComPtr<IAudioCaptureClient> capture;
    CHECK(client->GetService(IID_PPV_ARGS(&capture)));
    CHECK(client->Start());
    std::vector<short> packet(960);
    for (size_t i = 0; i < packet.size(); i++)
        packet[i] = static_cast<short>((static_cast<int>(i % 48) - 24) * 300); // known 1 kHz ramp, test only
    auto start = GetTickCount64(), next = start;
    unsigned int samples = 0, nonzero = 0, matches = 0, adjacent = 0, silence = 0;
    short previous = 0;
    bool havePrevious = false;
    while (GetTickCount64() - start < 4000) {
        const auto now = GetTickCount64();
        if (now - start < 2000 && now >= next) {
            if (!DeviceIoControl(control.value, TYU_MIC_IOCTL_WRITE, packet.data(),
                                 static_cast<DWORD>(packet.size() * 2), nullptr, 0, &returned, nullptr)) {
                std::puts("PCM injection failed");
                return 4;
            }
            next += 20;
        }
        if (now - start >= 2000) {
            if (!DeviceIoControl(control.value, TYU_MIC_IOCTL_RESET, nullptr, 0, nullptr, 0, &returned,
                                 nullptr))
                return 4;
        }
        UINT frames = 0;
        CHECK(capture->GetNextPacketSize(&frames));
        while (frames) {
            BYTE *data = nullptr;
            DWORD flags = 0;
            CHECK(capture->GetBuffer(&data, &frames, &flags, nullptr, nullptr));
            auto input = reinterpret_cast<const short *>(data);
            for (UINT i = 0; i < frames; i++) {
                const short value = (flags & AUDCLNT_BUFFERFLAGS_SILENT) ? 0 : input[i];
                if (now - start < 2000) {
                    samples++;
                    if (std::abs(static_cast<int>(value)) > 100)
                        nonzero++;
                    if (havePrevious && value && previous) {
                        adjacent++;
                        auto delta = static_cast<int>(value) - previous;
                        if (std::abs(delta - 300) < 15 || std::abs(delta + 14100) < 15)
                            matches++;
                    }
                    previous = value;
                    havePrevious = true;
                } else if (now - start > 2500) {
                    if (value != 0) {
                        std::puts("FAIL: audio remained after reset");
                        capture->ReleaseBuffer(frames);
                        client->Stop();
                        return 5;
                    }
                    silence++;
                }
            }
            CHECK(capture->ReleaseBuffer(frames));
            CHECK(capture->GetNextPacketSize(&frames));
        }
        Sleep(2);
    }
    CHECK(client->Stop());
    std::printf("TYU WASAPI samples=%u nonzero=%u ramp_matches=%u/%u "
                "silence_after_reset=%u\n",
                samples, nonzero, matches, adjacent, silence);
    if (nonzero < 24000 || adjacent < 24000 || matches * 100ULL < adjacent * 90ULL || silence < 24000)
        return 6;
    std::puts("PASS: real TYU capture endpoint received the injected waveform "
              "and reset to silence");
    return 0;
}
