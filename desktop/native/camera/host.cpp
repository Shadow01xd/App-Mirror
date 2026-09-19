#include "tyu_camera.h"
#include <sddl.h>
#include <io.h>
#include <fcntl.h>
#include <iostream>
#include <mfreadwrite.h>

struct FrameStore {
    std::mutex lock;
    std::vector<BYTE> frame;
    std::atomic<bool> running{true};
    FrameStore():frame(FRAME_BYTES,128) {std::fill(frame.begin(),frame.begin()+WIDTH*HEIGHT,16);}
};
static std::wstring PipeSecurity() {
    HANDLE token=nullptr;
    if(!OpenProcessToken(GetCurrentProcess(),TOKEN_QUERY,&token)) return {};
    DWORD size=0;GetTokenInformation(token,TokenUser,nullptr,0,&size);
    std::vector<BYTE> user(size);
    if(!GetTokenInformation(token,TokenUser,user.data(),size,&size)) {CloseHandle(token);return {};}
    CloseHandle(token);LPWSTR sid=nullptr;
    if(!ConvertSidToStringSidW(reinterpret_cast<TOKEN_USER*>(user.data())->User.Sid,&sid))return {};
    std::wstring s=L"D:P(A;;GA;;;SY)(A;;GA;;;LS)(A;;GA;;;";s+=sid;s+=L")";LocalFree(sid);return s;
}
static void Serve(const std::wstring& name,std::shared_ptr<FrameStore> frames) {
    auto sddl=PipeSecurity();PSECURITY_DESCRIPTOR descriptor=nullptr;
    if(!ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.c_str(),SDDL_REVISION_1,&descriptor,nullptr))return;
    SECURITY_ATTRIBUTES security{sizeof(security),descriptor,FALSE};
    std::atomic<int> clients{0};
    while(frames->running) {
        HANDLE pipe=CreateNamedPipeW(name.c_str(),PIPE_ACCESS_DUPLEX|FILE_FLAG_OVERLAPPED,PIPE_TYPE_BYTE|PIPE_READMODE_BYTE|PIPE_WAIT|PIPE_REJECT_REMOTE_CLIENTS,8,FRAME_BYTES,64,1000,&security);
        if(pipe==INVALID_HANDLE_VALUE)break;
        OVERLAPPED ov{};ov.hEvent=CreateEventW(nullptr,TRUE,FALSE,nullptr);
        bool connected=ConnectNamedPipe(pipe,&ov)!=FALSE;
        if(!connected && GetLastError()==ERROR_PIPE_CONNECTED)connected=true;
        else if(!connected && GetLastError()==ERROR_IO_PENDING) {
            while(frames->running && WaitForSingleObject(ov.hEvent,200)==WAIT_TIMEOUT) {}
            DWORD bytes=0;
            if(!frames->running) {CancelIoEx(pipe,&ov);WaitForSingleObject(ov.hEvent,INFINITE);}
            connected=GetOverlappedResult(pipe,&ov,&bytes,FALSE)!=FALSE;
        }
        CloseHandle(ov.hEvent);
        if(!connected) {CloseHandle(pipe);continue;}
        // Each client only reads the most recent frame; no unbounded camera queues.
        std::thread([pipe,frames] {
            std::vector<BYTE> snapshot(FRAME_BYTES);
            while(frames->running) {
                BYTE request=0;if(!PipeIO(pipe,&request,1,false)||request!=1)break;
                {std::lock_guard guard(frames->lock);snapshot=frames->frame;}
                DWORD size=FRAME_BYTES;
                if(!PipeIO(pipe,&size,4,true)||!PipeIO(pipe,snapshot.data(),size,true))break;
            }
            DisconnectNamedPipe(pipe);CloseHandle(pipe);
        }).detach();
    }
    LocalFree(descriptor);
}
static HRESULT SelfTest(const std::wstring& pipe,std::shared_ptr<FrameStore> frames) {
    WCHAR path[MAX_PATH]{};GetModuleFileNameW(nullptr,path,MAX_PATH);
    std::wstring dll=path;dll=dll.substr(0,dll.find_last_of(L"\\/")+1)+L"TyuCameraSource.dll";
    HMODULE module=LoadLibraryW(dll.c_str());if(!module)return HRESULT_FROM_WIN32(GetLastError());
    auto factory=reinterpret_cast<LPFNGETCLASSOBJECT>(GetProcAddress(module,"DllGetClassObject"));if(!factory)return E_NOINTERFACE;
    ComPtr<IClassFactory> creator;HR(factory(TYU_CAMERA_CLSID,IID_PPV_ARGS(&creator)));
    ComPtr<IMFActivate> activation;HR(creator->CreateInstance(nullptr,IID_PPV_ARGS(&activation)));
    HR(activation->SetString(TYU_CAMERA_PIPE,pipe.c_str()));
    ComPtr<IMFMediaSource> source;HR(activation->ActivateObject(IID_PPV_ARGS(&source)));
    {
        std::lock_guard guard(frames->lock);
        std::fill(frames->frame.begin(),frames->frame.begin()+WIDTH*HEIGHT,81);
        for(UINT32 i=WIDTH*HEIGHT;i<FRAME_BYTES;i+=2){frames->frame[i]=90;frames->frame[i+1]=240;}
    }
    ComPtr<IMFSourceReader> reader;HR(MFCreateSourceReaderFromMediaSource(source.Get(),nullptr,&reader));
    ComPtr<IMFMediaType> type;HR(reader->GetNativeMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM,0,&type));
    HR(reader->SetCurrentMediaType(MF_SOURCE_READER_FIRST_VIDEO_STREAM,nullptr,type.Get()));
    int pictures=0;
    for(int n=0;n<70;n++) {
        if(n==35)Sleep(1500); // Exercise consumers that pause long enough for an idle pipe to close.
        DWORD flags=0;LONGLONG time=0;ComPtr<IMFSample> sample;
        HR(reader->ReadSample(MF_SOURCE_READER_FIRST_VIDEO_STREAM,0,nullptr,&flags,&time,&sample));
        if(sample) {
            ComPtr<IMFMediaBuffer> buffer;HR(sample->ConvertToContiguousBuffer(&buffer));
            BYTE* pixels=nullptr;DWORD length=0;HR(buffer->Lock(&pixels,nullptr,&length));
            bool valid=length>=FRAME_BYTES&&pixels[0]==81&&pixels[WIDTH*HEIGHT]==90&&pixels[WIDTH*HEIGHT+1]==240;
            buffer->Unlock();if(!valid)return E_UNEXPECTED;
            pictures++;
        }
    }
    source->Shutdown();reader.Reset();activation->ShutdownObject();
    std::cout<<"SELF_TEST frames="<<pictures<<std::endl;
    return pictures>=65?S_OK:E_FAIL;
}
static HRESULT Run(bool test) {
    HR(CoInitializeEx(nullptr,COINIT_MULTITHREADED));HR(MFStartup(MF_VERSION));
    GUID id{};HR(CoCreateGuid(&id));WCHAR text[40]{};StringFromGUID2(id,text,40);
    std::wstring pipe=L"\\\\.\\pipe\\TYU-Camera-";pipe+=text;
    auto frames=std::make_shared<FrameStore>();
    std::thread server(Serve,pipe,frames);
    if(test) {
        Sleep(100);
        HRESULT result=SelfTest(pipe,frames);
        frames->running=false;server.join();MFShutdown();CoUninitialize();return result;
    }
    ComPtr<IMFVirtualCamera> camera;
    ComPtr<IMFActivate> installed;
    HRESULT hr=CoCreateInstance(TYU_CAMERA_CLSID,nullptr,CLSCTX_INPROC_SERVER,IID_PPV_ARGS(&installed));
    if(SUCCEEDED(hr)) {
        for(int attempt=0;attempt<3;attempt++) {
            hr=MFCreateVirtualCamera(MFVirtualCameraType_SoftwareCameraSource,MFVirtualCameraLifetime_Session,MFVirtualCameraAccess_CurrentUser,L"TYU Camera",TYU_CAMERA_ID,nullptr,0,&camera);
            if(SUCCEEDED(hr))hr=camera->SetString(TYU_CAMERA_PIPE,pipe.c_str());
            if(SUCCEEDED(hr))hr=camera->Start(nullptr);
            if(SUCCEEDED(hr))break;
            if(camera){camera->Remove();camera->Shutdown();camera.Reset();}
            // PnP can still be retiring the previous session during a quick stop/start.
            if(hr!=HRESULT_FROM_WIN32(ERROR_NOT_FOUND))break;
            Sleep(300);
        }
    }
    if(SUCCEEDED(hr)) {
        std::cout<<"READY"<<std::endl;
        _setmode(_fileno(stdin),_O_BINARY);
        std::vector<BYTE> next(FRAME_BYTES);
        for(;;) {
            UINT32 size=0;
            if(fread(&size,1,4,stdin)!=4)break;
            if(size!=FRAME_BYTES) {hr=E_INVALIDARG;break;}
            if(fread(next.data(),1,size,stdin)!=size)break;
            {std::lock_guard lock(frames->lock);frames->frame.swap(next);}
        }
        camera->Stop();camera->Remove();camera->Shutdown();
    }
    // Release COM objects before tearing down COM: CoUninitialize can unload the source DLL.
    frames->running=false;server.join();camera.Reset();installed.Reset();MFShutdown();CoUninitialize();return hr;
}
int wmain(int argc,wchar_t** argv) {auto hr=Run(argc>1&&wcscmp(argv[1],L"--self-test")==0);if(FAILED(hr)){std::cout<<"ERROR 0x"<<std::hex<<static_cast<unsigned long>(hr)<<std::endl;return 1;}return 0;}
