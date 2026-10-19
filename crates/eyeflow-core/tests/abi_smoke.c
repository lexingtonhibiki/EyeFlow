#include "eyeflow.h"
#include <assert.h>
#include <stdlib.h>

/* Compile against the release library to check the actual exported C boundary. */
int main(void) {
    EyeFlow *h = eyeflow_create();
    assert(h);
    assert(eyeflow_configure(h, 60, 30, 0, 900) == 0);
    assert(eyeflow_tick(h, 45000, 0, 0) == 1);
    eyeflow_command(h, 2);
    assert(eyeflow_postponed_count(h) == 1);
    size_t n = eyeflow_export_state(h, NULL, 0);
    assert(n > 0);
    uint8_t *bytes = malloc(n);
    assert(bytes);
    assert(eyeflow_export_state(h, bytes, n) == n);
    EyeFlow *restored = eyeflow_create();
    assert(eyeflow_import_state(restored, bytes, n) == 0);
    assert(eyeflow_remaining_seconds(restored) == 300);
    assert(eyeflow_tick(restored, 285000, 0, 0) == 1);
    eyeflow_command(restored, 1);
    assert(eyeflow_tick(restored, 315000, 0, 0) == 0);
    assert(eyeflow_completed_count(restored) == 1);
    assert(eyeflow_tick(restored, 0, 0, 0) == -1);
    free(bytes);
    eyeflow_destroy(restored);
    eyeflow_destroy(h);
    eyeflow_destroy(NULL);
    return 0;
}
