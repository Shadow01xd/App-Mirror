#include "pcm_ring.h"
#include "protocol.h"
#include <cassert>
#include <cstdio>
#include <vector>
int main() {
    TyuPcmRing ring;
    assert(ring.Read() == 0);
    unsigned char pcm[] = {0, 0, 255, 127, 0, 128, 255, 255};
    assert(ring.Write(pcm, sizeof(pcm)));
    assert(ring.Read() == 0);
    assert(ring.Read() == 32767);
    assert(ring.Read() == -32768);
    assert(ring.Read() == -1);
    assert(ring.Read() == 0);
    assert(!ring.Write(pcm, 3));
    assert(!ring.Write(nullptr, 2));
    assert(!ring.Write(pcm, TYU_MIC_MAX_SAMPLES * 2 + 2));
    // Wrap-around, regular 20 ms Android packets, and zero-fill after
    // disconnection.
    std::vector<unsigned char> packet(1920);
    for (unsigned int turn = 0; turn < 2000; turn++) {
        for (unsigned int i = 0; i < 960; i++) {
            packet[i * 2] = static_cast<unsigned char>(turn);
            packet[i * 2 + 1] = 0;
        }
        assert(ring.Write(packet.data(), static_cast<unsigned int>(packet.size())));
        for (unsigned int i = 0; i < 960; i++)
            assert(ring.Read() == static_cast<short>(turn & 255));
    }
    for (int i = 0; i < 5; i++)
        assert(ring.Write(packet.data(), 1920));
    assert(ring.Available() == TyuPcmRing::Capacity);
    assert(ring.Write(pcm, sizeof(pcm)));
    assert(ring.Available() == 4);
    ring.Clear();
    assert(ring.Available() == 0);
    assert(ring.Read() == 0);
    std::puts("TYU microphone PCM: signed samples, wrap, overflow, validation "
              "and disconnect passed");
}
