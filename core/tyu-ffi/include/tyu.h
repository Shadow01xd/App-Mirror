#ifndef TYU_H
#define TYU_H
#include <stdint.h>
#include <stddef.h>
#ifdef __cplusplus
extern "C" {
#endif
/* Control schema: postcard TyuCommand/TyuEvent v0.1, not yet a frozen public SDK.
 * Status: -1 invalid input, -2 invalid state, -3 limit, -4 failure, -5 busy.
 * All functions serialize operations per handle. Never call shutdown concurrently
 * with use of a media pointer. Call shutdown before unloading the library. */
uint64_t tyu_initialize(const uint8_t *directory, size_t length);
intptr_t tyu_start(uint64_t handle);
intptr_t tyu_send_command(uint64_t handle, const uint8_t *data, size_t length);
/* If capacity is too small, the positive required size is returned; event retained.
 * Null output + capacity zero queries required size. Poll controls regularly. */
intptr_t tyu_poll_event(uint64_t handle, uint8_t *output, size_t capacity);
typedef struct {
    uint64_t lease;
    const uint8_t *data;
    size_t length;
    uint8_t peer[16], session[16], stream[16];
} TyuMediaView;
/* Returns 1 with a lease, 0 when empty. At most four concurrent frame leases.
 * Pointers are read-only and remain valid until release or shutdown. */
intptr_t tyu_acquire_media(uint64_t handle, TyuMediaView *output);
intptr_t tyu_release_media(uint64_t handle, uint64_t lease);
intptr_t tyu_shutdown(uint64_t handle);
#ifdef __cplusplus
}
#endif
#endif
