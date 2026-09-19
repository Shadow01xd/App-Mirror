// Elevated installer for the single TYU root device. The PowerShell entry point
// verifies the signed catalog first.
#define UNICODE
#define _UNICODE
#include <windows.h>
#include <cstdio>
#include <devguid.h>
#include <newdev.h>
#include <setupapi.h>
#include <string>
#include <vector>
#pragma comment(lib, "setupapi.lib")
#pragma comment(lib, "newdev.lib")
static constexpr wchar_t Hardware[] = L"Root\\TYUMicrophone";
static bool IsTyu(HDEVINFO devices, SP_DEVINFO_DATA *device) {
    DWORD type = 0, bytes = 0;
    SetupDiGetDeviceRegistryPropertyW(devices, device, SPDRP_HARDWAREID, &type, nullptr, 0, &bytes);
    if (type != REG_MULTI_SZ || !bytes || bytes > 65536)
        return false;
    std::vector<wchar_t> ids(bytes / sizeof(wchar_t) + 2, 0);
    if (!SetupDiGetDeviceRegistryPropertyW(devices, device, SPDRP_HARDWAREID, &type,
                                           reinterpret_cast<PBYTE>(ids.data()), bytes, nullptr))
        return false;
    for (auto id = ids.data(); *id; id += wcslen(id) + 1)
        if (_wcsicmp(id, Hardware) == 0)
            return true;
    return false;
}
static DWORD Run(bool remove, const wchar_t *inf) {
    HDEVINFO devices = SetupDiGetClassDevsW(&GUID_DEVCLASS_MEDIA, nullptr, nullptr, 0);
    if (devices == INVALID_HANDLE_VALUE)
        return GetLastError();
    SP_DEVINFO_DATA device{sizeof(device)};
    bool found = false;
    for (DWORD i = 0; SetupDiEnumDeviceInfo(devices, i, &device); i++) {
        if (!IsTyu(devices, &device))
            continue;
        found = true;
        if (remove) {
            SP_REMOVEDEVICE_PARAMS parameters{};
            parameters.ClassInstallHeader.cbSize = sizeof(SP_CLASSINSTALL_HEADER);
            parameters.ClassInstallHeader.InstallFunction = DIF_REMOVE;
            parameters.Scope = DI_REMOVEDEVICE_GLOBAL;
            if (!SetupDiSetClassInstallParamsW(devices, &device, &parameters.ClassInstallHeader,
                                               sizeof(parameters)) ||
                !SetupDiCallClassInstaller(DIF_REMOVE, devices, &device)) {
                auto error = GetLastError();
                SetupDiDestroyDeviceInfoList(devices);
                return error;
            }
        }
    }
    SetupDiDestroyDeviceInfoList(devices);
    if (remove)
        return ERROR_SUCCESS;
    bool created = false;
    devices = SetupDiCreateDeviceInfoList(&GUID_DEVCLASS_MEDIA, nullptr);
    if (devices == INVALID_HANDLE_VALUE)
        return GetLastError();
    device = {sizeof(device)};
    if (!found) {
        if (!SetupDiCreateDeviceInfoW(devices, L"TYUMicrophone", &GUID_DEVCLASS_MEDIA, L"TYU Microphone",
                                      nullptr, DICD_GENERATE_ID, &device)) {
            auto error = GetLastError();
            SetupDiDestroyDeviceInfoList(devices);
            return error;
        }
        // Extra explicit NUL forms a MULTI_SZ with its required double terminator.
        const wchar_t ids[] = L"Root\\TYUMicrophone\0";
        if (!SetupDiSetDeviceRegistryPropertyW(devices, &device, SPDRP_HARDWAREID,
                                               reinterpret_cast<const BYTE *>(ids), sizeof(ids)) ||
            !SetupDiCallClassInstaller(DIF_REGISTERDEVICE, devices, &device)) {
            auto error = GetLastError();
            SetupDiDestroyDeviceInfoList(devices);
            return error;
        }
        created = true;
    }
    BOOL reboot = FALSE;
    if (!UpdateDriverForPlugAndPlayDevicesW(nullptr, Hardware, inf, INSTALLFLAG_NONINTERACTIVE, &reboot)) {
        auto error = GetLastError();
        if (created)
            SetupDiRemoveDevice(devices, &device);
        SetupDiDestroyDeviceInfoList(devices);
        return error;
    }
    SetupDiDestroyDeviceInfoList(devices);
    return reboot ? ERROR_SUCCESS_REBOOT_REQUIRED : ERROR_SUCCESS;
}
int wmain(int count, wchar_t **arguments) {
    if (count != 2) {
        std::fwprintf(stderr, L"Usage: TyuMicInstall.exe <absolute signed INF path | --uninstall>\n");
        return ERROR_INVALID_PARAMETER;
    }
    const bool remove = wcscmp(arguments[1], L"--uninstall") == 0;
    auto result = Run(remove, arguments[1]);
    std::wprintf(L"TYU microphone installation result: %lu\n", result);
    return static_cast<int>(result);
}
