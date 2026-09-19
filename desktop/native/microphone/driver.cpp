// TYU virtual microphone: PortCls WaveRT capture, fed by authenticated TYU
// Desktop sessions. No hardware access, network stack, synthetic tone, external
// audio cable or user pointers.
#include <ntddk.h>
#include <initguid.h>
#include <portcls.h>
#include <ks.h>
#include <ksmedia.h>
#include <wdmsec.h>
#include "pcm_ring.h"
#include "protocol.h"

constexpr ULONG Tag = 'muYT';
enum class NonPaged { Allocate };
void *operator new(size_t size, NonPaged) noexcept { return ExAllocatePool2(POOL_FLAG_NON_PAGED, size, Tag); }
void operator delete(void *p) noexcept {
    if (p)
        ExFreePoolWithTag(p, Tag);
}
void operator delete(void *p, size_t) noexcept {
    if (p)
        ExFreePoolWithTag(p, Tag);
}
void operator delete(void *p, NonPaged) noexcept {
    if (p)
        ExFreePoolWithTag(p, Tag);
}

static KSPIN_LOCK AudioLock;
static TyuPcmRing Audio;
static PDEVICE_OBJECT Control = nullptr;
static PFILE_OBJECT Writer = nullptr;
static LONG Ready = 0, Adapters = 0, Streams = 0;
static PDRIVER_DISPATCH Original[IRP_MJ_MAXIMUM_FUNCTION + 1]{};
static PDRIVER_UNLOAD OriginalUnload = nullptr;
static const GUID ControlClass = {
    0x8bc56d48, 0xcd54, 0x4c44, {0xb6, 0x28, 0x95, 0x15, 0x46, 0x77, 0x2e, 0x4c}};
static UNICODE_STRING DeviceName = RTL_CONSTANT_STRING(L"\\Device\\TYUMicrophone");
static UNICODE_STRING LinkName = RTL_CONSTANT_STRING(L"\\DosDevices\\TYUMicrophone");

static void ClearAudio() {
    KIRQL old;
    KeAcquireSpinLock(&AudioLock, &old);
    Audio.Clear();
    KeReleaseSpinLock(&AudioLock, old);
}
static NTSTATUS Complete(PIRP irp, NTSTATUS status, ULONG_PTR bytes = 0) {
    irp->IoStatus.Status = status;
    irp->IoStatus.Information = bytes;
    IoCompleteRequest(irp, IO_NO_INCREMENT);
    return status;
}

static KSDATARANGE_AUDIO PcmRange = {
    {sizeof(KSDATARANGE_AUDIO), 0, 0, 0, STATICGUIDOF(KSDATAFORMAT_TYPE_AUDIO),
     STATICGUIDOF(KSDATAFORMAT_SUBTYPE_PCM), STATICGUIDOF(KSDATAFORMAT_SPECIFIER_WAVEFORMATEX)},
    1,
    16,
    16,
    48000,
    48000};
static KSDATARANGE BridgeRange = {sizeof(KSDATARANGE),
                                  0,
                                  0,
                                  0,
                                  STATICGUIDOF(KSDATAFORMAT_TYPE_AUDIO),
                                  STATICGUIDOF(KSDATAFORMAT_SUBTYPE_ANALOG),
                                  STATICGUIDOF(KSDATAFORMAT_SPECIFIER_NONE)};
static PKSDATARANGE PcmRanges[] = {reinterpret_cast<PKSDATARANGE>(&PcmRange)};
static PKSDATARANGE BridgeRanges[] = {&BridgeRange};
static PCPIN_DESCRIPTOR WavePins[] = {
    {0,
     0,
     0,
     nullptr,
     {0, nullptr, 0, nullptr, 1, BridgeRanges, KSPIN_DATAFLOW_IN, KSPIN_COMMUNICATION_NONE, &KSCATEGORY_AUDIO,
      nullptr, 0}},
    {1,
     1,
     0,
     nullptr,
     {0, nullptr, 0, nullptr, 1, PcmRanges, KSPIN_DATAFLOW_OUT, KSPIN_COMMUNICATION_SINK, &KSCATEGORY_AUDIO,
      &KSAUDFNAME_RECORDING_CONTROL, 0}}};
static PCPIN_DESCRIPTOR TopologyPins[] = {{0,
                                           0,
                                           0,
                                           nullptr,
                                           {0, nullptr, 0, nullptr, 1, BridgeRanges, KSPIN_DATAFLOW_IN,
                                            KSPIN_COMMUNICATION_NONE, &KSNODETYPE_MICROPHONE, nullptr, 0}},
                                          {0,
                                           0,
                                           0,
                                           nullptr,
                                           {0, nullptr, 0, nullptr, 1, BridgeRanges, KSPIN_DATAFLOW_OUT,
                                            KSPIN_COMMUNICATION_NONE, &KSCATEGORY_AUDIO, nullptr, 0}}};
static PCNODE_DESCRIPTOR WaveNodes[] = {{0, nullptr, &KSNODETYPE_ADC, nullptr}};
static PCCONNECTION_DESCRIPTOR WaveConnections[] = {{PCFILTER_NODE, 0, 0, 1}, {0, 0, PCFILTER_NODE, 1}};
static PCCONNECTION_DESCRIPTOR TopologyConnections[] = {{PCFILTER_NODE, 0, PCFILTER_NODE, 1}};
static PCFILTER_DESCRIPTOR WaveDescriptor = {
    0,         nullptr, sizeof(PCPIN_DESCRIPTOR), 2, WavePins, sizeof(PCNODE_DESCRIPTOR), 1,
    WaveNodes, 2,       WaveConnections,          0, nullptr};
static PCFILTER_DESCRIPTOR TopologyDescriptor = {
    0, nullptr, sizeof(PCPIN_DESCRIPTOR), 2, TopologyPins, 0, 0, nullptr, 1, TopologyConnections, 0, nullptr};

static void FillFormat(PKSDATAFORMAT_WAVEFORMATEXTENSIBLE f) {
    RtlZeroMemory(f, sizeof(*f));
    f->DataFormat.FormatSize = sizeof(*f);
    f->DataFormat.SampleSize = 2;
    f->DataFormat.MajorFormat = KSDATAFORMAT_TYPE_AUDIO;
    f->DataFormat.SubFormat = KSDATAFORMAT_SUBTYPE_PCM;
    f->DataFormat.Specifier = KSDATAFORMAT_SPECIFIER_WAVEFORMATEX;
    auto &w = f->WaveFormatExt;
    w.Format.wFormatTag = WAVE_FORMAT_EXTENSIBLE;
    w.Format.nChannels = 1;
    w.Format.nSamplesPerSec = 48000;
    w.Format.nAvgBytesPerSec = 96000;
    w.Format.nBlockAlign = 2;
    w.Format.wBitsPerSample = 16;
    w.Format.cbSize = 22;
    w.Samples.wValidBitsPerSample = 16;
    w.dwChannelMask = KSAUDIO_SPEAKER_MONO;
    w.SubFormat = KSDATAFORMAT_SUBTYPE_PCM;
}
static bool ValidFormat(PKSDATAFORMAT f) {
    if (!f || f->FormatSize < sizeof(KSDATAFORMAT) + sizeof(WAVEFORMATEX) ||
        !IsEqualGUIDAligned(f->MajorFormat, KSDATAFORMAT_TYPE_AUDIO) ||
        !IsEqualGUIDAligned(f->SubFormat, KSDATAFORMAT_SUBTYPE_PCM) ||
        !IsEqualGUIDAligned(f->Specifier, KSDATAFORMAT_SPECIFIER_WAVEFORMATEX))
        return false;
    auto w = reinterpret_cast<PWAVEFORMATEX>(f + 1);
    if (w->nChannels != 1 || w->nSamplesPerSec != 48000 || w->wBitsPerSample != 16 || w->nBlockAlign != 2 ||
        w->nAvgBytesPerSec != 96000)
        return false;
    if (w->wFormatTag == WAVE_FORMAT_PCM)
        return true;
    if (w->wFormatTag != WAVE_FORMAT_EXTENSIBLE ||
        f->FormatSize < sizeof(KSDATAFORMAT_WAVEFORMATEXTENSIBLE) || w->cbSize < 22)
        return false;
    auto x = reinterpret_cast<PWAVEFORMATEXTENSIBLE>(w);
    return x->Samples.wValidBitsPerSample == 16 && IsEqualGUIDAligned(x->SubFormat, KSDATAFORMAT_SUBTYPE_PCM);
}

#define REFCOUNT                                                                                             \
    STDMETHODIMP_(ULONG) AddRef() override { return static_cast<ULONG>(InterlockedIncrement(&references)); } \
    STDMETHODIMP_(ULONG) Release() override {                                                                \
        auto value = InterlockedDecrement(&references);                                                      \
        if (!value)                                                                                          \
            delete this;                                                                                     \
        return static_cast<ULONG>(value);                                                                    \
    }

class Capture final : public IMiniportWaveRTStreamNotification, public IMiniportWaveRTInputStream {
    LONG references = 1;
    PPORTWAVERTSTREAM port;
    KSPIN_LOCK lock;
    PEX_TIMER timer = nullptr;
    KSSTATE state = KSSTATE_STOP;
    PMDL mdl = nullptr;
    PUCHAR buffer = nullptr;
    ULONG size = 0, notifications = 0;
    ULONGLONG position = 0, origin = 0, base = 0;
    ULONG lastPacket = MAXULONG;
    PKEVENT events[8]{};
    _Function_class_(EXT_CALLBACK) _IRQL_requires_(DISPATCH_LEVEL) _IRQL_requires_same_
        static void Tick(_In_ PEX_TIMER, _In_opt_ PVOID context) {
        if (!context)
            return;
        auto self = static_cast<Capture *>(context);
        KIRQL old;
        KeAcquireSpinLock(&self->lock, &old);
        self->Advance();
        KeReleaseSpinLock(&self->lock, old);
    }
    void Advance() {
        // lock held at DISPATCH_LEVEL. AudioLock is always acquired after this
        // lock.
        if (state != KSSTATE_RUN || !buffer || !size)
            return;
        const auto elapsed = KeQueryInterruptTime() - origin;
        const auto samples = (elapsed / 10000000) * 48000 + (elapsed % 10000000) * 48000 / 10000000;
        const auto target = base + samples * 2;
        if (target <= position)
            return;
        const auto packetBytes = notifications ? size / notifications : size;
        const auto previousPacket = position / packetBytes;
        auto bytes = target - position;
        KeAcquireSpinLockAtDpcLevel(&AudioLock);
        if (bytes > size) {
            Audio.Clear();
            position = target - size;
            bytes = size;
        }
        for (ULONGLONG n = 0; n < bytes; n += 2) {
            const auto sample = static_cast<USHORT>(Audio.Read());
            const auto index = static_cast<ULONG>(position % size);
            buffer[index] = static_cast<UCHAR>(sample);
            buffer[index + 1] = static_cast<UCHAR>(sample >> 8);
            position += 2;
        }
        KeReleaseSpinLockFromDpcLevel(&AudioLock);
        KeMemoryBarrier();
        if (notifications && position / packetBytes != previousPacket)
            for (auto event : events)
                if (event)
                    KeSetEvent(event, IO_NO_INCREMENT, FALSE);
    }
    void Halt() {
        KIRQL old;
        KeAcquireSpinLock(&lock, &old);
        Advance();
        state = KSSTATE_STOP;
        if (timer)
            ExCancelTimer(timer, nullptr);
        KeReleaseSpinLock(&lock, old);
    }

  public:
    explicit Capture(PPORTWAVERTSTREAM value) : port(value) {
        port->AddRef();
        KeInitializeSpinLock(&lock);
        timer = ExAllocateTimer(Tick, this, EX_TIMER_HIGH_RESOLUTION);
    }
    bool Initialized() const { return timer != nullptr; }
    ~Capture() {
        Halt();
        if (timer)
            ExDeleteTimer(timer, TRUE, TRUE, nullptr);
        if (mdl) {
            port->UnmapAllocatedPages(buffer, mdl);
            port->FreePagesFromMdl(mdl);
        }
        port->Release();
        InterlockedExchange(&Streams, 0);
    }
    REFCOUNT
    STDMETHODIMP QueryInterface(REFIID id, PVOID *out) override {
        if (!out)
            return STATUS_INVALID_PARAMETER;
        *out = nullptr;
        if (IsEqualGUIDAligned(id, IID_IUnknown) || IsEqualGUIDAligned(id, IID_IMiniportWaveRTStream) ||
            IsEqualGUIDAligned(id, IID_IMiniportWaveRTStreamNotification))
            *out = static_cast<IMiniportWaveRTStreamNotification *>(this);
        else if (IsEqualGUIDAligned(id, IID_IMiniportWaveRTInputStream))
            *out = static_cast<IMiniportWaveRTInputStream *>(this);
        else
            return STATUS_NOINTERFACE;
        AddRef();
        return STATUS_SUCCESS;
    }
    STDMETHODIMP SetFormat(PKSDATAFORMAT format) override {
        return ValidFormat(format) ? STATUS_SUCCESS : STATUS_NO_MATCH;
    }
    STDMETHODIMP SetState(KSSTATE next) override {
        if (next < KSSTATE_STOP || next > KSSTATE_RUN)
            return STATUS_INVALID_PARAMETER;
        KIRQL old;
        KeAcquireSpinLock(&lock, &old);
        Advance();
        if (next == KSSTATE_RUN && (!buffer || !size)) {
            KeReleaseSpinLock(&lock, old);
            return STATUS_INVALID_DEVICE_STATE;
        }
        if (next == KSSTATE_RUN && state != KSSTATE_RUN) {
            KeAcquireSpinLockAtDpcLevel(&AudioLock);
            Audio.Clear();
            KeReleaseSpinLockFromDpcLevel(&AudioLock);
            base = position;
            origin = KeQueryInterruptTime();
            state = next;
            ExSetTimer(timer, -10000, 10000, nullptr);
        } else {
            state = next;
            if (next != KSSTATE_RUN)
                ExCancelTimer(timer, nullptr);
            if (next == KSSTATE_STOP) {
                position = 0;
                base = 0;
                lastPacket = MAXULONG;
                if (buffer)
                    RtlZeroMemory(buffer, size);
            }
        }
        KeReleaseSpinLock(&lock, old);
        return STATUS_SUCCESS;
    }
    STDMETHODIMP GetPosition(PKSAUDIO_POSITION out) override {
        if (!out)
            return STATUS_INVALID_PARAMETER;
        KIRQL old;
        KeAcquireSpinLock(&lock, &old);
        Advance();
        out->PlayOffset = size ? position % size : 0;
        out->WriteOffset = out->PlayOffset;
        KeReleaseSpinLock(&lock, old);
        return STATUS_SUCCESS;
    }
    STDMETHODIMP AllocateAudioBuffer(ULONG requested, PMDL *out, ULONG *actual, ULONG *offset,
                                     MEMORY_CACHING_TYPE *cache) override {
        if (!out || !actual || !offset || !cache)
            return STATUS_INVALID_PARAMETER;
        *out = nullptr;
        if (mdl || requested < 192 || requested > 192000 || (requested & 1))
            return STATUS_INVALID_PARAMETER;
        PHYSICAL_ADDRESS high;
        high.QuadPart = MAXLONGLONG;
        auto allocation = port->AllocatePagesForMdl(high, requested);
        if (!allocation)
            return STATUS_INSUFFICIENT_RESOURCES;
        auto mapped = static_cast<PUCHAR>(port->MapAllocatedPages(allocation, MmCached));
        if (!mapped) {
            port->FreePagesFromMdl(allocation);
            return STATUS_INSUFFICIENT_RESOURCES;
        }
        RtlZeroMemory(mapped, requested);
        KIRQL old;
        KeAcquireSpinLock(&lock, &old);
        mdl = allocation;
        buffer = mapped;
        size = requested;
        position = 0;
        KeReleaseSpinLock(&lock, old);
        *out = allocation;
        *actual = requested;
        *offset = 0;
        *cache = MmCached;
        return STATUS_SUCCESS;
    }
    STDMETHODIMP_(void) FreeAudioBuffer(PMDL value, ULONG) override {
        Halt();
        if (!value || value != mdl)
            return;
        auto mapped = buffer;
        KIRQL old;
        KeAcquireSpinLock(&lock, &old);
        buffer = nullptr;
        mdl = nullptr;
        size = 0;
        notifications = 0;
        KeReleaseSpinLock(&lock, old);
        port->UnmapAllocatedPages(mapped, value);
        port->FreePagesFromMdl(value);
    }
    STDMETHODIMP_(void) GetHWLatency(PKSRTAUDIO_HWLATENCY out) override {
        if (out)
            RtlZeroMemory(out, sizeof(*out));
    }
    STDMETHODIMP GetPositionRegister(PKSRTAUDIO_HWREGISTER) override { return STATUS_NOT_SUPPORTED; }
    STDMETHODIMP GetClockRegister(PKSRTAUDIO_HWREGISTER) override { return STATUS_NOT_SUPPORTED; }
    STDMETHODIMP
    AllocateBufferWithNotification(ULONG count, ULONG requested, PMDL *out, ULONG *actual, ULONG *offset,
                                   MEMORY_CACHING_TYPE *cache) override {
        if ((count != 1 && count != 2) || requested % (count * 2))
            return STATUS_INVALID_PARAMETER;
        auto status = AllocateAudioBuffer(requested, out, actual, offset, cache);
        if (NT_SUCCESS(status))
            notifications = count;
        return status;
    }
    STDMETHODIMP_(void)
    FreeBufferWithNotification(PMDL value, ULONG length) override { FreeAudioBuffer(value, length); }
    STDMETHODIMP RegisterNotificationEvent(PKEVENT event) override {
        if (!event)
            return STATUS_INVALID_PARAMETER;
        KIRQL old;
        KeAcquireSpinLock(&lock, &old);
        NTSTATUS status = STATUS_INSUFFICIENT_RESOURCES;
        bool duplicate = false;
        for (auto current : events)
            if (current == event)
                duplicate = true;
        if (duplicate)
            status = STATUS_OBJECT_NAME_COLLISION;
        else
            for (auto &current : events)
                if (!current) {
                    current = event;
                    status = STATUS_SUCCESS;
                    break;
                }
        KeReleaseSpinLock(&lock, old);
        return status;
    }
    STDMETHODIMP UnregisterNotificationEvent(PKEVENT event) override {
        KIRQL old;
        KeAcquireSpinLock(&lock, &old);
        NTSTATUS status = STATUS_NOT_FOUND;
        for (auto &current : events)
            if (current == event) {
                current = nullptr;
                status = STATUS_SUCCESS;
                break;
            }
        KeReleaseSpinLock(&lock, old);
        return status;
    }
    STDMETHODIMP GetReadPacket(ULONG *packet, DWORD *flags, ULONG64 *qpc, BOOL *more) override {
        if (!packet || !flags || !qpc || !more)
            return STATUS_INVALID_PARAMETER;
        KIRQL old;
        KeAcquireSpinLock(&lock, &old);
        Advance();
        if (!size || !notifications || position < size / notifications) {
            KeReleaseSpinLock(&lock, old);
            return STATUS_DEVICE_NOT_READY;
        }
        const auto fullPacket = position / (size / notifications) - 1;
        auto latest = static_cast<ULONG>(fullPacket);
        if (latest == lastPacket) {
            KeReleaseSpinLock(&lock, old);
            return STATUS_DEVICE_NOT_READY;
        }
        *packet = latest;
        *flags = 0;
        *more = FALSE;
        LARGE_INTEGER frequency;
        auto now = KeQueryPerformanceCounter(&frequency);
        const auto afterStart = position - fullPacket * (size / notifications);
        *qpc = now.QuadPart - (afterStart * frequency.QuadPart / 96000);
        lastPacket = latest;
        KeReleaseSpinLock(&lock, old);
        return STATUS_SUCCESS;
    }
};

class Wave final : public IMiniportWaveRT {
    LONG references = 1;

  public:
    REFCOUNT
    STDMETHODIMP QueryInterface(REFIID id, PVOID *out) override {
        if (!out)
            return STATUS_INVALID_PARAMETER;
        *out = nullptr;
        if (!IsEqualGUIDAligned(id, IID_IUnknown) && !IsEqualGUIDAligned(id, IID_IMiniport) &&
            !IsEqualGUIDAligned(id, IID_IMiniportWaveRT))
            return STATUS_NOINTERFACE;
        *out = static_cast<IMiniportWaveRT *>(this);
        AddRef();
        return STATUS_SUCCESS;
    }
    STDMETHODIMP Init(PUNKNOWN, PRESOURCELIST, PPORTWAVERT) override { return STATUS_SUCCESS; }
    STDMETHODIMP GetDescription(PPCFILTER_DESCRIPTOR *out) override {
        if (!out)
            return STATUS_INVALID_PARAMETER;
        *out = &WaveDescriptor;
        return STATUS_SUCCESS;
    }
    STDMETHODIMP GetDeviceDescription(PDEVICE_DESCRIPTION out) override {
        if (!out)
            return STATUS_INVALID_PARAMETER;
        RtlZeroMemory(out, sizeof(*out));
        out->Version = DEVICE_DESCRIPTION_VERSION;
        out->Master = TRUE;
        out->ScatterGather = TRUE;
        out->Dma32BitAddresses = TRUE;
        out->InterfaceType = PCIBus;
        out->MaximumLength = 192000;
        return STATUS_SUCCESS;
    }
    STDMETHODIMP NewStream(PMINIPORTWAVERTSTREAM *out, PPORTWAVERTSTREAM port, ULONG pin, BOOLEAN capture,
                           PKSDATAFORMAT format) override {
        if (!out || !port)
            return STATUS_INVALID_PARAMETER;
        *out = nullptr;
        if (pin != 1 || !capture || !ValidFormat(format))
            return STATUS_NO_MATCH;
        if (InterlockedCompareExchange(&Streams, 1, 0) != 0)
            return STATUS_DEVICE_BUSY;
        auto stream = new (NonPaged::Allocate) Capture(port);
        if (!stream) {
            InterlockedExchange(&Streams, 0);
            return STATUS_INSUFFICIENT_RESOURCES;
        }
        if (!stream->Initialized()) {
            delete stream;
            return STATUS_INSUFFICIENT_RESOURCES;
        }
        *out = static_cast<IMiniportWaveRTStreamNotification *>(stream);
        return STATUS_SUCCESS;
    }
    STDMETHODIMP DataRangeIntersection(ULONG pin, PKSDATARANGE client, PKSDATARANGE, ULONG length,
                                       PVOID output, PULONG required) override {
        if (!client || !required || pin != 1 || client->FormatSize < sizeof(KSDATARANGE_AUDIO))
            return STATUS_NO_MATCH;
        if (!IsEqualGUIDAligned(client->MajorFormat, KSDATAFORMAT_TYPE_AUDIO) ||
            !IsEqualGUIDAligned(client->SubFormat, KSDATAFORMAT_SUBTYPE_PCM) ||
            !IsEqualGUIDAligned(client->Specifier, KSDATAFORMAT_SPECIFIER_WAVEFORMATEX))
            return STATUS_NO_MATCH;
        auto range = reinterpret_cast<PKSDATARANGE_AUDIO>(client);
        if (range->MaximumChannels < 1 || range->MinimumBitsPerSample > 16 ||
            range->MaximumBitsPerSample < 16 || range->MinimumSampleFrequency > 48000 ||
            range->MaximumSampleFrequency < 48000)
            return STATUS_NO_MATCH;
        *required = sizeof(KSDATAFORMAT_WAVEFORMATEXTENSIBLE);
        if (!length)
            return STATUS_BUFFER_OVERFLOW;
        if (length < *required)
            return STATUS_BUFFER_TOO_SMALL;
        if (!output)
            return STATUS_INVALID_PARAMETER;
        FillFormat(static_cast<PKSDATAFORMAT_WAVEFORMATEXTENSIBLE>(output));
        return STATUS_SUCCESS;
    }
};
class Topology final : public IMiniportTopology {
    LONG references = 1;

  public:
    REFCOUNT
    STDMETHODIMP QueryInterface(REFIID id, PVOID *out) override {
        if (!out)
            return STATUS_INVALID_PARAMETER;
        *out = nullptr;
        if (!IsEqualGUIDAligned(id, IID_IUnknown) && !IsEqualGUIDAligned(id, IID_IMiniport) &&
            !IsEqualGUIDAligned(id, IID_IMiniportTopology))
            return STATUS_NOINTERFACE;
        *out = static_cast<IMiniportTopology *>(this);
        AddRef();
        return STATUS_SUCCESS;
    }
    STDMETHODIMP Init(PUNKNOWN, PRESOURCELIST, PPORTTOPOLOGY) override { return STATUS_SUCCESS; }
    STDMETHODIMP GetDescription(PPCFILTER_DESCRIPTOR *out) override {
        if (!out)
            return STATUS_INVALID_PARAMETER;
        *out = &TopologyDescriptor;
        return STATUS_SUCCESS;
    }
    STDMETHODIMP DataRangeIntersection(ULONG, PKSDATARANGE, PKSDATARANGE, ULONG, PVOID, PULONG) override {
        return STATUS_NOT_IMPLEMENTED;
    }
};

static NTSTATUS Register(PDEVICE_OBJECT device, PIRP irp, PCWSTR name, REFCLSID kind, PMINIPORT mini,
                         PRESOURCELIST resources, PPORT *out) {
    PPORT port = nullptr;
    auto status = PcNewPort(&port, kind);
    if (NT_SUCCESS(status))
        status = port->Init(device, irp, mini, nullptr, resources);
    if (NT_SUCCESS(status))
        status = PcRegisterSubdevice(device, const_cast<PWSTR>(name), port);
    if (NT_SUCCESS(status))
        *out = port;
    else if (port)
        port->Release();
    return status;
}
static NTSTATUS StartDevice(PDEVICE_OBJECT device, PIRP irp, PRESOURCELIST resources) {
    // PortCls retains initialized miniports through COM AddRef. The initial
    // reference is released on every path below; analysis cannot follow Release.
#pragma warning(suppress : 28197)
    auto wave = new (NonPaged::Allocate) Wave();
#pragma warning(suppress : 28197)
    auto topology = new (NonPaged::Allocate) Topology();
    if (!wave || !topology) {
        if (wave)
            wave->Release();
        if (topology)
            topology->Release();
        return STATUS_INSUFFICIENT_RESOURCES;
    }
    PPORT wavePort = nullptr, topologyPort = nullptr;
    auto status = Register(device, irp, L"Topology", CLSID_PortTopology, topology, resources, &topologyPort);
    if (NT_SUCCESS(status))
        status = Register(device, irp, L"Wave", CLSID_PortWaveRT, wave, resources, &wavePort);
    if (NT_SUCCESS(status))
        status = PcRegisterPhysicalConnection(device, topologyPort, 1, wavePort, 0);
    if (wavePort)
        wavePort->Release();
    if (topologyPort)
        topologyPort->Release();
    wave->Release();
    topology->Release();
    if (NT_SUCCESS(status)) {
        ClearAudio();
        InterlockedExchange(&Ready, 1);
    }
    return status;
}
static DRIVER_ADD_DEVICE AddDevice;
_Use_decl_annotations_ static NTSTATUS AddDevice(PDRIVER_OBJECT driver, PDEVICE_OBJECT physical) {
    if (InterlockedCompareExchange(&Adapters, 1, 0) != 0)
        return STATUS_DEVICE_BUSY;
    auto status = PcAddAdapterDevice(driver, physical, StartDevice, 2, 0);
    if (!NT_SUCCESS(status))
        InterlockedExchange(&Adapters, 0);
    return status;
}
static DRIVER_DISPATCH Dispatch;
_Use_decl_annotations_ static NTSTATUS Dispatch(PDEVICE_OBJECT device, PIRP irp) {
    auto stack = IoGetCurrentIrpStackLocation(irp);
    auto major = stack->MajorFunction;
    if (device != Control) {
        if (major == IRP_MJ_PNP &&
            (stack->MinorFunction == IRP_MN_STOP_DEVICE || stack->MinorFunction == IRP_MN_SURPRISE_REMOVAL ||
             stack->MinorFunction == IRP_MN_REMOVE_DEVICE)) {
            InterlockedExchange(&Ready, 0);
            ClearAudio();
            if (stack->MinorFunction == IRP_MN_REMOVE_DEVICE)
                InterlockedExchange(&Adapters, 0);
        }
        if (major == IRP_MJ_POWER && stack->MinorFunction == IRP_MN_SET_POWER &&
            stack->Parameters.Power.Type == DevicePowerState)
            ClearAudio();
        return Original[major](device, irp);
    }
    if (major == IRP_MJ_CREATE) {
        if (stack->FileObject->FileName.Length)
            return Complete(irp, STATUS_OBJECT_NAME_NOT_FOUND);
        KIRQL old;
        KeAcquireSpinLock(&AudioLock, &old);
        const auto status = Writer ? STATUS_SHARING_VIOLATION : STATUS_SUCCESS;
        if (NT_SUCCESS(status)) {
            Writer = stack->FileObject;
            Audio.Clear();
        }
        KeReleaseSpinLock(&AudioLock, old);
        return Complete(irp, status);
    }
    if (major == IRP_MJ_CLEANUP || major == IRP_MJ_CLOSE) {
        KIRQL old;
        KeAcquireSpinLock(&AudioLock, &old);
        if (Writer == stack->FileObject) {
            Audio.Clear();
            Writer = nullptr;
        }
        KeReleaseSpinLock(&AudioLock, old);
        return Complete(irp, STATUS_SUCCESS);
    }
    if (major != IRP_MJ_DEVICE_CONTROL)
        return Complete(irp, STATUS_INVALID_DEVICE_REQUEST);
    const auto &request = stack->Parameters.DeviceIoControl;
    if (request.IoControlCode == TYU_MIC_IOCTL_INFO) {
        if (request.OutputBufferLength < sizeof(TyuMicInfo))
            return Complete(irp, STATUS_BUFFER_TOO_SMALL);
        auto info = static_cast<TyuMicInfo *>(irp->AssociatedIrp.SystemBuffer);
        *info = {TYU_MIC_VERSION,
                 48000,
                 1,
                 16,
                 static_cast<unsigned int>(InterlockedCompareExchange(&Ready, 0, 0)),
                 0};
        return Complete(irp, STATUS_SUCCESS, sizeof(*info));
    }
    KIRQL old;
    KeAcquireSpinLock(&AudioLock, &old);
    NTSTATUS status = STATUS_INVALID_DEVICE_REQUEST;
    if (Writer != stack->FileObject)
        status = STATUS_ACCESS_DENIED;
    else if (request.IoControlCode == TYU_MIC_IOCTL_RESET) {
        Audio.Clear();
        status = STATUS_SUCCESS;
    } else if (request.IoControlCode == TYU_MIC_IOCTL_WRITE) {
        if (!InterlockedCompareExchange(&Ready, 0, 0))
            status = STATUS_DEVICE_NOT_READY;
        else
            status = Audio.Write(static_cast<const unsigned char *>(irp->AssociatedIrp.SystemBuffer),
                                 request.InputBufferLength)
                         ? STATUS_SUCCESS
                         : STATUS_INVALID_BUFFER_SIZE;
    }
    KeReleaseSpinLock(&AudioLock, old);
    return Complete(irp, status);
}
static DRIVER_UNLOAD Unload;
_Use_decl_annotations_ static void Unload(PDRIVER_OBJECT driver) {
    InterlockedExchange(&Ready, 0);
    ClearAudio();
    IoDeleteSymbolicLink(&LinkName);
    if (Control) {
        IoDeleteDevice(Control);
        Control = nullptr;
    }
    if (OriginalUnload)
        OriginalUnload(driver);
}
extern "C" DRIVER_INITIALIZE DriverEntry;
extern "C" NTSTATUS DriverEntry(PDRIVER_OBJECT driver, PUNICODE_STRING registry) {
    KeInitializeSpinLock(&AudioLock);
    Audio.Clear();
    auto status = PcInitializeAdapterDriver(driver, registry, AddDevice);
    if (!NT_SUCCESS(status))
        return status;
    // Interactive local users may feed their session; only one writer may hold
    // the handle.
    UNICODE_STRING acl = RTL_CONSTANT_STRING(L"D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;IU)");
    status = IoCreateDeviceSecure(driver, 0, &DeviceName, TYU_MIC_DEVICE_TYPE, FILE_DEVICE_SECURE_OPEN, FALSE,
                                  &acl, &ControlClass, &Control);
    if (!NT_SUCCESS(status))
        return status;
    status = IoCreateSymbolicLink(&LinkName, &DeviceName);
    if (!NT_SUCCESS(status)) {
        IoDeleteDevice(Control);
        Control = nullptr;
        return status;
    }
    for (ULONG i = 0; i <= IRP_MJ_MAXIMUM_FUNCTION; i++) {
        Original[i] = driver->MajorFunction[i];
        driver->MajorFunction[i] = Dispatch;
    }
    OriginalUnload = driver->DriverUnload;
    driver->DriverUnload = Unload;
    Control->Flags &= ~DO_DEVICE_INITIALIZING;
    return STATUS_SUCCESS;
}
