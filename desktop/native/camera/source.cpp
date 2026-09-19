// TYU Camera: native Windows 11 Media Foundation software camera, no third-party application.
#include "tyu_camera.h"
#include <ks.h>
#include <ksmedia.h>
#include <ksproxy.h>
#include <propvarutil.h>
#include <shlwapi.h>

#define EVENTS \
 HRESULT STDMETHODCALLTYPE GetEvent(DWORD f,IMFMediaEvent** e) override {return events?events->GetEvent(f,e):MF_E_SHUTDOWN;} \
 HRESULT STDMETHODCALLTYPE BeginGetEvent(IMFAsyncCallback* c,IUnknown* s) override {return events?events->BeginGetEvent(c,s):MF_E_SHUTDOWN;} \
 HRESULT STDMETHODCALLTYPE EndGetEvent(IMFAsyncResult* r,IMFMediaEvent** e) override {return events?events->EndGetEvent(r,e):MF_E_SHUTDOWN;} \
 HRESULT STDMETHODCALLTYPE QueueEvent(MediaEventType t,REFGUID g,HRESULT h,const PROPVARIANT* v) override {return events?events->QueueEventParamVar(t,g,h,v):MF_E_SHUTDOWN;}

class Stream final: public RuntimeClass<RuntimeClassFlags<ClassicCom>,Microsoft::WRL::ChainInterfaces<IMFMediaStream2,IMFMediaStream,IMFMediaEventGenerator>,IMFAsyncCallback> {
    ComPtr<IMFMediaSource> parent;
    ComPtr<IMFMediaEventQueue> events;
    ComPtr<IMFVideoSampleAllocator> allocator;
    ComPtr<IMFMediaType> type;
    std::wstring pipeName;
    std::atomic<MF_STREAM_STATE> state{MF_STREAM_STATE_STOPPED};
    std::atomic<bool> shutdown{false};
    DWORD workQueue=0;
    std::mutex frameLock;
    HANDLE pipe=INVALID_HANDLE_VALUE;
    LONGLONG previous=0;
public:
    ComPtr<IMFStreamDescriptor> descriptor;
    ComPtr<IMFAttributes> attributes;
    ~Stream() {if(pipe!=INVALID_HANDLE_VALUE) CloseHandle(pipe);if(workQueue) MFUnlockWorkQueue(workQueue);}
    HRESULT Init(IMFMediaSource* owner,const std::wstring& name) {
        parent=owner;pipeName=name;
        HR(MFCreateEventQueue(&events));HR(MFAllocateWorkQueueEx(MF_MULTITHREADED_WORKQUEUE,&workQueue));
        HR(MFCreateMediaType(&type));
        HR(type->SetGUID(MF_MT_MAJOR_TYPE,MFMediaType_Video));HR(type->SetGUID(MF_MT_SUBTYPE,MFVideoFormat_NV12));
        HR(type->SetUINT32(MF_MT_INTERLACE_MODE,MFVideoInterlace_Progressive));
        HR(type->SetUINT32(MF_MT_ALL_SAMPLES_INDEPENDENT,TRUE));
        HR(type->SetUINT32(MF_MT_FIXED_SIZE_SAMPLES,TRUE));HR(type->SetUINT32(MF_MT_SAMPLE_SIZE,FRAME_BYTES));
        HR(type->SetUINT32(MF_MT_DEFAULT_STRIDE,WIDTH));HR(MFSetAttributeSize(type.Get(),MF_MT_FRAME_SIZE,WIDTH,HEIGHT));
        HR(MFSetAttributeRatio(type.Get(),MF_MT_FRAME_RATE,30,1));HR(MFSetAttributeRatio(type.Get(),MF_MT_PIXEL_ASPECT_RATIO,1,1));
        IMFMediaType* types[]={type.Get()};HR(MFCreateStreamDescriptor(0,1,types,&descriptor));
        ComPtr<IMFMediaTypeHandler> handler;HR(descriptor->GetMediaTypeHandler(&handler));HR(handler->SetCurrentMediaType(type.Get()));
        HR(MFCreateAttributes(&attributes,4));
        HR(attributes->SetGUID(MF_DEVICESTREAM_STREAM_CATEGORY,PINNAME_VIDEO_CAPTURE));
        HR(attributes->SetUINT32(MF_DEVICESTREAM_STREAM_ID,0));
        HR(attributes->SetUINT32(MF_DEVICESTREAM_FRAMESERVER_SHARED,1));
        HR(attributes->SetUINT32(MF_DEVICESTREAM_ATTRIBUTE_FRAMESOURCE_TYPES,MFFrameSourceTypes_Color));
        return attributes->CopyAllItems(descriptor.Get());
    }
    EVENTS
    HRESULT STDMETHODCALLTYPE GetMediaSource(IMFMediaSource** out) override {return parent.CopyTo(out);}
    HRESULT STDMETHODCALLTYPE GetStreamDescriptor(IMFStreamDescriptor** out) override {return descriptor.CopyTo(out);}
    HRESULT STDMETHODCALLTYPE SetStreamState(MF_STREAM_STATE s) override {state=s;return S_OK;}
    HRESULT STDMETHODCALLTYPE GetStreamState(MF_STREAM_STATE* s) override {if(!s)return E_POINTER;*s=state;return S_OK;}
    HRESULT STDMETHODCALLTYPE RequestSample(IUnknown* token) override {
        if(shutdown) return MF_E_SHUTDOWN;
        if(state!=MF_STREAM_STATE_RUNNING) return MF_E_INVALIDREQUEST;
        return MFPutWorkItem(workQueue,this,token);
    }
    HRESULT STDMETHODCALLTYPE GetParameters(DWORD*,DWORD*) override {return E_NOTIMPL;}
    HRESULT STDMETHODCALLTYPE Invoke(IMFAsyncResult* result) override {
        std::lock_guard lock(frameLock);
        if(shutdown || state!=MF_STREAM_STATE_RUNNING) return S_OK;
        auto now=MFGetSystemTime();
        if(previous && now<previous+333333) Sleep(static_cast<DWORD>((previous+333333-now)/10000));
        previous=MFGetSystemTime();
        ComPtr<IUnknown> token;result->GetState(&token);
        HRESULT hr=Deliver(token.Get());
        if(FAILED(hr) && !shutdown) events->QueueEventParamVar(MEError,GUID_NULL,hr,nullptr);
        return S_OK;
    }
    HRESULT SetAllocator(IUnknown* value) {return value?value->QueryInterface(IID_PPV_ARGS(&allocator)):E_POINTER;}
    HRESULT Start() {
        std::lock_guard lock(frameLock);
        if(shutdown) return MF_E_SHUTDOWN;
        if(!allocator) HR(MFCreateVideoSampleAllocatorEx(IID_PPV_ARGS(&allocator)));
        HR(allocator->InitializeSampleAllocator(5,type.Get()));
        previous=0;state=MF_STREAM_STATE_RUNNING;
        PROPVARIANT v{};InitPropVariantFromInt64(0,&v);
        return events->QueueEventParamVar(MEStreamStarted,GUID_NULL,S_OK,&v);
    }
    HRESULT Stop() {state=MF_STREAM_STATE_STOPPED;return events->QueueEventParamVar(MEStreamStopped,GUID_NULL,S_OK,nullptr);}
    HRESULT Shutdown() {
        shutdown=true;state=MF_STREAM_STATE_STOPPED;
        std::lock_guard lock(frameLock);
        parent.Reset();if(allocator)allocator->UninitializeSampleAllocator();
        return events?events->Shutdown():S_OK;
    }
private:
    HRESULT Deliver(IUnknown* token) {
        std::vector<BYTE> frame(FRAME_BYTES,0);
        // Neutral black until TYU supplies its first phone frame; never retain a previous user's image.
        std::fill(frame.begin(),frame.begin()+WIDTH*HEIGHT,16);
        std::fill(frame.begin()+WIDTH*HEIGHT,frame.end(),128);
        for(int attempt=0;attempt<2;attempt++) {
            if(pipe==INVALID_HANDLE_VALUE) pipe=CreateFileW(pipeName.c_str(),GENERIC_READ|GENERIC_WRITE,0,nullptr,OPEN_EXISTING,FILE_FLAG_OVERLAPPED,nullptr);
            if(pipe==INVALID_HANDLE_VALUE)break;
            BYTE request=1;DWORD size=0;
            if(PipeIO(pipe,&request,1,true)&&PipeIO(pipe,&size,4,false)&&size==FRAME_BYTES&&PipeIO(pipe,frame.data(),FRAME_BYTES,false))break;
            // Frame Server can keep the source alive while a consumer pauses. Reopen an idle pipe.
            CloseHandle(pipe);pipe=INVALID_HANDLE_VALUE;
            std::fill(frame.begin(),frame.begin()+WIDTH*HEIGHT,16);
            std::fill(frame.begin()+WIDTH*HEIGHT,frame.end(),128);
        }
        ComPtr<IMFSample> sample;HR(allocator->AllocateSample(&sample));
        ComPtr<IMFMediaBuffer> buffer;HR(sample->GetBufferByIndex(0,&buffer));
        ComPtr<IMF2DBuffer> two;
        if(SUCCEEDED(buffer.As(&two))) {
            BYTE* output=nullptr;LONG pitch=0;HR(two->Lock2D(&output,&pitch));
            if(pitch<static_cast<LONG>(WIDTH)) {two->Unlock2D();return E_UNEXPECTED;}
            for(UINT32 y=0;y<HEIGHT*3/2;y++) memcpy(output+y*pitch,frame.data()+y*WIDTH,WIDTH);
            HR(two->Unlock2D());
        } else {
            BYTE* output=nullptr;DWORD maximum=0;HR(buffer->Lock(&output,&maximum,nullptr));
            if(maximum<FRAME_BYTES) {buffer->Unlock();return E_UNEXPECTED;}
            memcpy(output,frame.data(),FRAME_BYTES);HR(buffer->Unlock());HR(buffer->SetCurrentLength(FRAME_BYTES));
        }
        HR(sample->SetSampleTime(previous));HR(sample->SetSampleDuration(333333));
        if(token) HR(sample->SetUnknown(MFSampleExtension_Token,token));
        return events->QueueEventParamUnk(MEMediaSample,GUID_NULL,S_OK,sample.Get());
    }
};

class Source final: public RuntimeClass<RuntimeClassFlags<ClassicCom>,Microsoft::WRL::ChainInterfaces<IMFMediaSourceEx,IMFMediaSource,IMFMediaEventGenerator>,IMFGetService,IKsControl,IMFSampleAllocatorControl> {
    ComPtr<IMFMediaEventQueue> events;
    ComPtr<IMFAttributes> attributes;
    ComPtr<IMFPresentationDescriptor> presentation;
    ComPtr<Stream> stream;
    bool announced=false;
    std::atomic<bool> shutdown{false};
public:
    HRESULT Init(IMFAttributes* activation) {
        HR(MFCreateEventQueue(&events));HR(MFCreateAttributes(&attributes,8));
        HR(activation->CopyAllItems(attributes.Get()));
        HR(attributes->SetUINT32(MF_DEVICESTREAM_ATTRIBUTE_FRAMESOURCE_TYPES,MFFrameSourceTypes_Color));
        WCHAR name[256]{};HR(activation->GetString(TYU_CAMERA_PIPE,name,256,nullptr));
        if(wcsncmp(name,L"\\\\.\\pipe\\TYU-Camera-",20)!=0) return E_INVALIDARG;
        stream=Make<Stream>();if(!stream)return E_OUTOFMEMORY;
        HR(stream->Init(this,name));IMFStreamDescriptor* descriptors[]={stream->descriptor.Get()};
        HR(MFCreatePresentationDescriptor(1,descriptors,&presentation));return presentation->SelectStream(0);
    }
    EVENTS
    HRESULT STDMETHODCALLTYPE GetCharacteristics(DWORD* flags) override {if(!flags)return E_POINTER;*flags=MFMEDIASOURCE_IS_LIVE;return shutdown?MF_E_SHUTDOWN:S_OK;}
    HRESULT STDMETHODCALLTYPE CreatePresentationDescriptor(IMFPresentationDescriptor** out) override {return shutdown?MF_E_SHUTDOWN:presentation->Clone(out);}
    HRESULT STDMETHODCALLTYPE Start(IMFPresentationDescriptor* pd,const GUID* format,const PROPVARIANT* position) override {
        if(shutdown)return MF_E_SHUTDOWN;if(!pd||!position)return E_INVALIDARG;
        if(format&&*format!=GUID_NULL)return MF_E_UNSUPPORTED_TIME_FORMAT;
        DWORD count=0;HR(pd->GetStreamDescriptorCount(&count));if(count!=1)return E_INVALIDARG;
        BOOL selected=FALSE;ComPtr<IMFStreamDescriptor> desc;HR(pd->GetStreamDescriptorByIndex(0,&selected,&desc));if(!selected)return MF_E_INVALIDREQUEST;
        HR(events->QueueEventParamUnk(announced?MEUpdatedStream:MENewStream,GUID_NULL,S_OK,static_cast<IMFMediaStream2*>(stream.Get())));
        announced=true;HR(stream->Start());
        PROPVARIANT v{};InitPropVariantFromInt64(0,&v);
        return events->QueueEventParamVar(MESourceStarted,GUID_NULL,S_OK,&v);
    }
    HRESULT STDMETHODCALLTYPE Stop() override {if(shutdown)return MF_E_SHUTDOWN;HR(stream->Stop());return events->QueueEventParamVar(MESourceStopped,GUID_NULL,S_OK,nullptr);}
    HRESULT STDMETHODCALLTYPE Pause() override {return MF_E_INVALID_STATE_TRANSITION;}
    HRESULT STDMETHODCALLTYPE Shutdown() override {if(shutdown.exchange(true))return S_OK;if(stream)stream->Shutdown();stream.Reset();return events?events->Shutdown():S_OK;}
    HRESULT STDMETHODCALLTYPE GetSourceAttributes(IMFAttributes** out) override {return attributes.CopyTo(out);}
    HRESULT STDMETHODCALLTYPE GetStreamAttributes(DWORD id,IMFAttributes** out) override {if(shutdown)return MF_E_SHUTDOWN;return id?MF_E_INVALIDSTREAMNUMBER:stream->attributes.CopyTo(out);}
    HRESULT STDMETHODCALLTYPE SetD3DManager(IUnknown*) override {return E_NOTIMPL;}
    HRESULT STDMETHODCALLTYPE GetService(REFGUID,REFIID,void** out) override {if(out)*out=nullptr;return MF_E_UNSUPPORTED_SERVICE;}
    HRESULT STDMETHODCALLTYPE KsProperty(PKSPROPERTY,ULONG,void*,ULONG,ULONG* bytes) override {if(bytes)*bytes=0;return HRESULT_FROM_WIN32(ERROR_SET_NOT_FOUND);}
    HRESULT STDMETHODCALLTYPE KsMethod(PKSMETHOD,ULONG,void*,ULONG,ULONG* bytes) override {if(bytes)*bytes=0;return HRESULT_FROM_WIN32(ERROR_SET_NOT_FOUND);}
    HRESULT STDMETHODCALLTYPE KsEvent(PKSEVENT,ULONG,void*,ULONG,ULONG* bytes) override {if(bytes)*bytes=0;return HRESULT_FROM_WIN32(ERROR_SET_NOT_FOUND);}
    HRESULT STDMETHODCALLTYPE SetDefaultAllocator(DWORD id,IUnknown* value) override {if(shutdown)return MF_E_SHUTDOWN;return id?MF_E_INVALIDSTREAMNUMBER:stream->SetAllocator(value);}
    HRESULT STDMETHODCALLTYPE GetAllocatorUsage(DWORD id,DWORD* input,MFSampleAllocatorUsage* usage) override {
        if(id)return MF_E_INVALIDSTREAMNUMBER;if(!input||!usage)return E_POINTER;*input=0;*usage=MFSampleAllocatorUsage_UsesProvidedAllocator;return S_OK;
    }
};

class Activate final: public RuntimeClass<RuntimeClassFlags<ClassicCom>,Microsoft::WRL::ChainInterfaces<IMFActivate,IMFAttributes>> {
    ComPtr<IMFAttributes> attrs;
    ComPtr<Source> source;
public:
    HRESULT Init() {return MFCreateAttributes(&attrs,8);}
    HRESULT STDMETHODCALLTYPE ActivateObject(REFIID iid,void** out) override {
        if(!out)return E_POINTER;*out=nullptr;
        if(!source){auto candidate=Make<Source>();if(!candidate)return E_OUTOFMEMORY;
            auto hr=candidate->Init(attrs.Get());if(FAILED(hr)){candidate->Shutdown();return hr;}source=candidate;}
        return source->QueryInterface(iid,out);
    }
    HRESULT STDMETHODCALLTYPE ShutdownObject() override {if(source)source->Shutdown();source.Reset();return S_OK;}
    HRESULT STDMETHODCALLTYPE DetachObject() override {source.Reset();return S_OK;}
#define ATTR(name,signature,args) HRESULT STDMETHODCALLTYPE name signature override {return attrs->name args;}
    ATTR(GetItem,(REFGUID k,PROPVARIANT* v),(k,v))
    ATTR(GetItemType,(REFGUID k,MF_ATTRIBUTE_TYPE* t),(k,t))
    ATTR(CompareItem,(REFGUID k,REFPROPVARIANT v,BOOL* b),(k,v,b))
    ATTR(Compare,(IMFAttributes* a,MF_ATTRIBUTES_MATCH_TYPE t,BOOL* b),(a,t,b))
    ATTR(GetUINT32,(REFGUID k,UINT32* v),(k,v))
    ATTR(GetUINT64,(REFGUID k,UINT64* v),(k,v))
    ATTR(GetDouble,(REFGUID k,double* v),(k,v))
    ATTR(GetGUID,(REFGUID k,GUID* v),(k,v))
    ATTR(GetStringLength,(REFGUID k,UINT32* n),(k,n))
    ATTR(GetString,(REFGUID k,LPWSTR v,UINT32 n,UINT32* r),(k,v,n,r))
    ATTR(GetAllocatedString,(REFGUID k,LPWSTR* v,UINT32* n),(k,v,n))
    ATTR(GetBlobSize,(REFGUID k,UINT32* n),(k,n))
    ATTR(GetBlob,(REFGUID k,UINT8* v,UINT32 n,UINT32* r),(k,v,n,r))
    ATTR(GetAllocatedBlob,(REFGUID k,UINT8** v,UINT32* n),(k,v,n))
    ATTR(GetUnknown,(REFGUID k,REFIID i,void** v),(k,i,v))
    ATTR(SetItem,(REFGUID k,REFPROPVARIANT v),(k,v))
    ATTR(DeleteItem,(REFGUID k),(k))
    ATTR(DeleteAllItems,(),())
    ATTR(SetUINT32,(REFGUID k,UINT32 v),(k,v))
    ATTR(SetUINT64,(REFGUID k,UINT64 v),(k,v))
    ATTR(SetDouble,(REFGUID k,double v),(k,v))
    ATTR(SetGUID,(REFGUID k,REFGUID v),(k,v))
    ATTR(SetString,(REFGUID k,LPCWSTR v),(k,v))
    ATTR(SetBlob,(REFGUID k,const UINT8* v,UINT32 n),(k,v,n))
    ATTR(SetUnknown,(REFGUID k,IUnknown* v),(k,v))
    ATTR(LockStore,(),())
    ATTR(UnlockStore,(),())
    ATTR(GetCount,(UINT32* n),(n))
    ATTR(GetItemByIndex,(UINT32 i,GUID* k,PROPVARIANT* v),(i,k,v))
    ATTR(CopyAllItems,(IMFAttributes* a),(a))
#undef ATTR
};
class Factory final: public RuntimeClass<RuntimeClassFlags<ClassicCom>,IClassFactory> {
    HRESULT STDMETHODCALLTYPE CreateInstance(IUnknown* outer,REFIID iid,void** out) override {if(outer)return CLASS_E_NOAGGREGATION;auto object=Make<Activate>();if(!object)return E_OUTOFMEMORY;HR(object->Init());return object->QueryInterface(iid,out);}
    HRESULT STDMETHODCALLTYPE LockServer(BOOL) override {return S_OK;}
};
static HMODULE module=nullptr;
BOOL WINAPI DllMain(HMODULE h,DWORD reason,LPVOID) {if(reason==DLL_PROCESS_ATTACH){module=h;DisableThreadLibraryCalls(h);}return TRUE;}
HRESULT __stdcall DllGetClassObject(REFCLSID clsid,REFIID iid,void** out) {if(clsid!=TYU_CAMERA_CLSID)return CLASS_E_CLASSNOTAVAILABLE;auto f=Make<Factory>();return f?f->QueryInterface(iid,out):E_OUTOFMEMORY;}
HRESULT __stdcall DllCanUnloadNow() {return S_FALSE;}
extern "C" HRESULT __declspec(dllexport) __stdcall DllRegisterServer() {
    WCHAR path[MAX_PATH]{};if(!GetModuleFileNameW(module,path,MAX_PATH))return HRESULT_FROM_WIN32(GetLastError());
    std::wstring key=L"SOFTWARE\\Classes\\CLSID\\";key+=TYU_CAMERA_ID;key+=L"\\InprocServer32";
    HKEY registry=nullptr;auto code=RegCreateKeyExW(HKEY_LOCAL_MACHINE,key.c_str(),0,nullptr,0,KEY_WRITE,nullptr,&registry,nullptr);if(code)return HRESULT_FROM_WIN32(code);
    code=RegSetValueExW(registry,nullptr,0,REG_SZ,reinterpret_cast<BYTE*>(path),static_cast<DWORD>((wcslen(path)+1)*2));
    if(!code) code=RegSetValueExW(registry,L"ThreadingModel",0,REG_SZ,reinterpret_cast<const BYTE*>(L"Both"),10);
    RegCloseKey(registry);return HRESULT_FROM_WIN32(code);
}
extern "C" HRESULT __declspec(dllexport) __stdcall DllUnregisterServer() {std::wstring key=L"SOFTWARE\\Classes\\CLSID\\";key+=TYU_CAMERA_ID;auto code=RegDeleteTreeW(HKEY_LOCAL_MACHINE,key.c_str());return code==ERROR_FILE_NOT_FOUND?S_OK:HRESULT_FROM_WIN32(code);}
