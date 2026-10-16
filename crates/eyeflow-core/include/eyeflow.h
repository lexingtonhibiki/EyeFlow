#ifndef EYEFLOW_H
#define EYEFLOW_H
#include <stdint.h>
#include <stddef.h>
#ifdef __cplusplus
extern "C" {
#endif
typedef struct EyeFlow EyeFlow;
/* Serialize all calls for a handle; never use a destroyed handle. */
EyeFlow *eyeflow_create(void);
void eyeflow_destroy(EyeFlow *handle);
/* 1 start, 2 postpone, 3 skip, 4 pause one hour, 5 resume.
 * Commands use the last tick's clock. Tick immediately before a command. */
void eyeflow_command(EyeFlow *handle, uint32_t command);
/* Caller-owned monotonic ACTIVE time since create. Stop advancing while locked.
 * focus != 0 suppresses interruptions. 0 idle, 1 heads-up, 2 break, -1 error.
 * Backward time is rejected without changing state. */
int32_t eyeflow_tick(EyeFlow *handle, uint64_t elapsed_ms, uint64_t idle_secs, uint8_t focus);
uint64_t eyeflow_remaining_seconds(const EyeFlow *handle);
uint32_t eyeflow_completed_count(const EyeFlow *handle);
uint32_t eyeflow_postponed_count(const EyeFlow *handle);
/* UTF-8 JSON bytes, no trailing NUL. Query required length with NULL,0.
 * Writes nothing when capacity is insufficient. Host owns storage.
 * Import restores config, statistics and remaining ACTIVE timer durations.
 * Time while the process is absent/locked is not counted.
 * Successful import rebases elapsed_ms to zero; restart the host active clock. */
size_t eyeflow_export_state(const EyeFlow *handle, uint8_t *buffer, size_t capacity);
int32_t eyeflow_import_state(EyeFlow *handle, const uint8_t *buffer, size_t length);
/* Set fixed short interval/duration and long interval/duration in seconds.
 * Values are bounded; zero long_after_secs disables long breaks. */
int32_t eyeflow_configure(EyeFlow *handle, uint64_t interval_secs, uint64_t break_secs,
                         uint64_t long_after_secs, uint64_t long_break_secs);
/* Mobile API: this handle and interval contract are NOT interchangeable with
 * the legacy desktop-compatible EyeFlow API above. */
typedef struct EyeFlowMobile EyeFlowMobile;
enum EyeFlowMobileState {
    EYEFLOW_MOBILE_TRACKING = 0,
    EYEFLOW_MOBILE_OUT_OF_SCOPE = 1,
    EYEFLOW_MOBILE_LOCKED = 2,
    EYEFLOW_MOBILE_UNKNOWN = 3,
    EYEFLOW_MOBILE_STOPPED = 4,
    EYEFLOW_MOBILE_VOLUNTARY_REST = 5
};
enum EyeFlowMobilePhase {
    EYEFLOW_MOBILE_IDLE = 0,
    EYEFLOW_MOBILE_SHORT_HEADS_UP = 1,
    EYEFLOW_MOBILE_SHORT_DUE = 2,
    EYEFLOW_MOBILE_LONG_HEADS_UP = 3,
    EYEFLOW_MOBILE_LONG_DUE = 4,
    EYEFLOW_MOBILE_SHORT_REST = 5,
    EYEFLOW_MOBILE_LONG_REST = 6
};
enum EyeFlowMobileCommand {
    EYEFLOW_MOBILE_START_REST = 1,
    EYEFLOW_MOBILE_POSTPONE = 2,
    EYEFLOW_MOBILE_SKIP = 3,
    EYEFLOW_MOBILE_PAUSE_ONE_HOUR = 4,
    EYEFLOW_MOBILE_RESUME = 5,
    EYEFLOW_MOBILE_STOP = 6,
    EYEFLOW_MOBILE_CANCEL_REST = 7
};
EyeFlowMobile *eyeflow_mobile_create(void);
void eyeflow_mobile_destroy(EyeFlowMobile *handle);
/* Consume a disjoint chronological interval, not a cumulative active clock.
 * Zero-duration calls mark state boundaries/update delivery eligibility.
 * The host MUST use UNKNOWN for incomplete event history, permission loss,
 * clock discontinuities, process gaps and ambiguous foreground state.
 * Only TRACKING accrues exposure. OUT_OF_SCOPE/UNKNOWN/STOPPED preserve debt.
 * LOCKED requires evidence covering the entire interval; interrupted chains
 * cannot be added. Default lock >=30s resets short once; >=900s clears long.
 * VOLUNTARY_REST counts only after START_REST and only while the own-app rest
 * UI is foreground/user-active. Target use/unknown/out-of-scope cancels it.
 * Guided and locked evidence are not combined to manufacture a complete rest.
 * Due reminders NEVER become completed rests merely with elapsed target use.
 * Delivery denial only suppresses output, never consumes debt or pending work.
 * Returns phase 0..6, or -1 for null handle/invalid state/delta >100 years. */
int32_t eyeflow_mobile_advance(EyeFlowMobile *handle, uint64_t delta_ms,
                             uint32_t state, uint8_t delivery_allowed);
/* Returns 0 on accepted command, -1 when invalid/inapplicable. Pause affects
 * delivery only: selected exposure still accrues; all supplied monotonic elapsed
 * intervals expire the pause. STOP preserves debt and disables accumulation
 * until RESUME. No command itself proves or credits rest. */
int32_t eyeflow_mobile_command(EyeFlowMobile *handle, uint32_t command);
int32_t eyeflow_mobile_phase(const EyeFlowMobile *handle);
int32_t eyeflow_mobile_state(const EyeFlowMobile *handle);
/* Remaining rounds up; debts/lock durations round down. */
uint64_t eyeflow_mobile_remaining_seconds(const EyeFlowMobile *handle);
uint64_t eyeflow_mobile_short_debt_seconds(const EyeFlowMobile *handle);
uint64_t eyeflow_mobile_long_debt_seconds(const EyeFlowMobile *handle);
uint64_t eyeflow_mobile_lock_seconds(const EyeFlowMobile *handle);
uint64_t eyeflow_mobile_paused_seconds(const EyeFlowMobile *handle);
/* Stable per reminder attempt, preserved across restore; 0 when none pending.
 * Combine with phase for host notification deduplication; failed delivery must
 * not be marked delivered by the host. Postpone creates a new delivery attempt. */
uint64_t eyeflow_mobile_reminder_id(const EyeFlowMobile *handle);
uint8_t eyeflow_mobile_can_deliver(const EyeFlowMobile *handle);
uint8_t eyeflow_mobile_can_postpone(const EyeFlowMobile *handle);
uint8_t eyeflow_mobile_enabled(const EyeFlowMobile *handle);
/* Daily counters saturate instead of wrapping. Completed includes guided short,
 * guided long and natural lock chains. Natural-long is an upgrade of an already
 * counted natural chain, not a second completed rest. */
uint32_t eyeflow_mobile_completed_count(const EyeFlowMobile *handle);
uint32_t eyeflow_mobile_postponed_count(const EyeFlowMobile *handle);
uint32_t eyeflow_mobile_skipped_count(const EyeFlowMobile *handle);
uint32_t eyeflow_mobile_natural_count(const EyeFlowMobile *handle);
uint32_t eyeflow_mobile_natural_long_count(const EyeFlowMobile *handle);
uint32_t eyeflow_mobile_guided_short_count(const EyeFlowMobile *handle);
uint32_t eyeflow_mobile_guided_long_count(const EyeFlowMobile *handle);
/* UTF-8 JSON, no NUL; query size with NULL,0. Insufficient buffers unchanged.
 * Mobile snapshots are distinct from legacy snapshots. Import is atomic and
 * preserves exposure, countdowns, pending reminders, postpone state and stats.
 * Import rebases to UNKNOWN (or STOPPED), cancels guided sessions and invalidates
 * lock continuity. It grants NO credit for process absence. Historical intervals
 * may be replayed only when independently proved; otherwise use UNKNOWN. */
size_t eyeflow_mobile_export_state(const EyeFlowMobile *handle, uint8_t *buffer, size_t capacity);
int32_t eyeflow_mobile_import_state(EyeFlowMobile *handle, const uint8_t *buffer, size_t length);
/* Optional fixed-interval configuration for tests/demo. Defaults otherwise use
 * triangular 15..25min (peak20), short30s, long900s after7200s selected exposure.
 * Interval/long-after <=86400s, rest <=3600s, long rest >=short rest.
 * Zero long-after disables long reminders. Invalid settings do not mutate state. */
int32_t eyeflow_mobile_configure(EyeFlowMobile *handle, uint64_t interval_secs,
                                uint64_t break_secs, uint64_t long_after_secs,
                                uint64_t long_break_secs);

#ifdef __cplusplus
}
#endif
#endif
