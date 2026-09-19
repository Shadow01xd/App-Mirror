#pragma once
// No allocation, CRT, floating-point or kernel dependencies. Callers serialize
// access. 100 ms hard limit; overflow discards old audio rather than increasing
// latency.
class TyuPcmRing {
  public:
    static constexpr unsigned int Capacity = 4800;
    void Clear() {
        for (unsigned int i = 0; i < Capacity; i++)
            samples[i] = 0;
        head = 0;
        count = 0;
    }
    bool Write(const unsigned char *bytes, unsigned int length) {
        if (!bytes || !length || (length & 1) || length > Capacity * 2)
            return false;
        const auto incoming = length / 2;
        if (count + incoming > Capacity)
            Clear();
        for (unsigned int i = 0; i < incoming; i++) {
            samples[(head + count) % Capacity] =
                static_cast<short>(bytes[2 * i] | (static_cast<unsigned int>(bytes[2 * i + 1]) << 8));
            count++;
        }
        return true;
    }
    short Read() {
        if (!count)
            return 0;
        auto sample = samples[head];
        samples[head] = 0;
        head = (head + 1) % Capacity;
        count--;
        return sample;
    }
    unsigned int Available() const { return count; }

  private:
    short samples[Capacity]{};
    unsigned int head = 0, count = 0;
};
