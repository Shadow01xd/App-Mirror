#pragma once
#define NOMINMAX
#include <windows.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mferror.h>
#include <mfvirtualcamera.h>
#include <wrl.h>
#include <string>
#include <vector>
#include <mutex>
#include <atomic>
#include <chrono>
#include <thread>
#include <memory>
using Microsoft::WRL::ComPtr;
using Microsoft::WRL::RuntimeClass;
using Microsoft::WRL::RuntimeClassFlags;
using Microsoft::WRL::ClassicCom;
using Microsoft::WRL::Make;
inline constexpr GUID TYU_CAMERA_CLSID = {0x9e125cb7,0xa9aa,0x4a82,{0x9e,0x12,0xa2,0xdf,0x17,0xf4,0x3a,0x51}};
inline constexpr GUID TYU_CAMERA_PIPE = {0xf016440c,0x4ba9,0x420f,{0xa1,0x13,0x87,0x44,0xa9,0x8f,0x02,0x36}};
inline constexpr wchar_t TYU_CAMERA_ID[] = L"{9E125CB7-A9AA-4A82-9E12-A2DF17F43A51}";
struct CameraFormat {
    UINT32 width=1280, height=720, fps=30;
    UINT32 bytes() const { return width*height*3/2; }
    LONGLONG duration() const { return 10000000/fps; }
    bool valid() const { return ((width==1280 && height==720)||(width==1920 && height==1080)) && (fps==30||fps==60); }
    HRESULT Store(IMFAttributes* attributes) const {
        auto hr=MFSetAttributeSize(attributes,MF_MT_FRAME_SIZE,width,height);
        return FAILED(hr)?hr:MFSetAttributeRatio(attributes,MF_MT_FRAME_RATE,fps,1);
    }
    HRESULT Load(IMFAttributes* attributes) {
        UINT32 denominator=1;
        auto hr=MFGetAttributeSize(attributes,MF_MT_FRAME_SIZE,&width,&height);
        if(hr==MF_E_ATTRIBUTENOTFOUND) {width=1280;height=720;fps=30;return S_OK;}
        if(FAILED(hr))return hr;
        hr=MFGetAttributeRatio(attributes,MF_MT_FRAME_RATE,&fps,&denominator);
        return FAILED(hr)?hr:valid()&&denominator==1?S_OK:E_INVALIDARG;
    }
};
#define HR(expr) do {HRESULT result=(expr); if(FAILED(result)) return result;} while(0)
// Bounded overlapped pipe I/O so a stopped TYU process cannot hang the camera Frame Server.
inline bool PipeIO(HANDLE pipe, void* data, DWORD size, bool write) {
    auto bytes=static_cast<BYTE*>(data);
    while(size) {
        OVERLAPPED ov{};ov.hEvent=CreateEventW(nullptr,TRUE,FALSE,nullptr);
        if(!ov.hEvent) return false;
        DWORD transferred=0;
        BOOL ok=write?WriteFile(pipe,bytes,size,&transferred,&ov):ReadFile(pipe,bytes,size,&transferred,&ov);
        if(!ok && GetLastError()==ERROR_IO_PENDING) {
            if(WaitForSingleObject(ov.hEvent,1000)!=WAIT_OBJECT_0) {CancelIoEx(pipe,&ov);WaitForSingleObject(ov.hEvent,INFINITE);}
            ok=GetOverlappedResult(pipe,&ov,&transferred,FALSE);
        }
        CloseHandle(ov.hEvent);
        if(!ok || !transferred) return false;
        bytes+=transferred;size-=transferred;
    }
    return true;
}
