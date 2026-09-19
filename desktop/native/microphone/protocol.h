#pragma once
// Stable wire contract between TYU Desktop and the TYU capture driver. PCM is
// S16LE mono.
#define TYU_MIC_VERSION 1u
#define TYU_MIC_RATE 48000u
#define TYU_MIC_MAX_SAMPLES 4800u
#define TYU_MIC_DEVICE_TYPE 0x8000u
// METHOD_BUFFERED, FILE_READ_DATA | FILE_WRITE_DATA. No caller-owned pointers
// enter the driver.
#define TYU_MIC_IOCTL_INFO 0x8000e000u
#define TYU_MIC_IOCTL_WRITE 0x8000e004u
#define TYU_MIC_IOCTL_RESET 0x8000e008u
struct TyuMicInfo {
    unsigned int version;
    unsigned int sampleRate;
    unsigned int channels;
    unsigned int bits;
    unsigned int ready;
    unsigned int reserved;
};
static_assert(sizeof(TyuMicInfo) == 24);
