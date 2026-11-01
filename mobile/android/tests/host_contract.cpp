// Actual mobile C ABI contract; not an Android OS/instrumentation test.
#include "eyeflow.h"
#include <cassert>
#include <iostream>
int main() {
    auto* h = eyeflow_mobile_create();
    assert(h);
    assert(eyeflow_mobile_configure(h, 60, 30, 7200, 900) == 0);
    eyeflow_mobile_advance(h, 70000, EYEFLOW_MOBILE_TRACKING, 1);
    auto debt = eyeflow_mobile_short_debt_seconds(h);
    assert(debt >= 60);
    assert(eyeflow_mobile_completed_count(h) == 0); // Reminder elapsed is NOT rest.
    eyeflow_mobile_advance(h, 600000, EYEFLOW_MOBILE_OUT_OF_SCOPE, 0);
    assert(eyeflow_mobile_short_debt_seconds(h) == debt);
    assert(eyeflow_mobile_natural_count(h) == 0);
    eyeflow_mobile_advance(h, 600000, EYEFLOW_MOBILE_UNKNOWN, 0);
    assert(eyeflow_mobile_short_debt_seconds(h) == debt);
    eyeflow_mobile_advance(h, 29000, EYEFLOW_MOBILE_LOCKED, 0);
    assert(eyeflow_mobile_short_debt_seconds(h) == debt);
    eyeflow_mobile_advance(h, 0, EYEFLOW_MOBILE_TRACKING, 0);
    eyeflow_mobile_advance(h, 1000, EYEFLOW_MOBILE_LOCKED, 0);
    assert(eyeflow_mobile_short_debt_seconds(h) == debt); // Split locks cannot add.
    eyeflow_mobile_advance(h, 29000, EYEFLOW_MOBILE_LOCKED, 0);
    assert(eyeflow_mobile_short_debt_seconds(h) == 0);
    auto longDebt = eyeflow_mobile_long_debt_seconds(h);
    assert(longDebt > 0);
    eyeflow_mobile_advance(h, 870000, EYEFLOW_MOBILE_LOCKED, 0);
    assert(eyeflow_mobile_long_debt_seconds(h) == 0);
    eyeflow_mobile_advance(h, 60000, EYEFLOW_MOBILE_TRACKING, 0);
    assert(eyeflow_mobile_command(h, EYEFLOW_MOBILE_START_REST) == 0);
    eyeflow_mobile_advance(h, 30000, EYEFLOW_MOBILE_TRACKING, 0);
    assert(eyeflow_mobile_short_debt_seconds(h) > 0); // Continuing target aborts rest.
    eyeflow_mobile_command(h, EYEFLOW_MOBILE_STOP);
    debt = eyeflow_mobile_short_debt_seconds(h);
    eyeflow_mobile_advance(h, 900000, EYEFLOW_MOBILE_LOCKED, 0);
    assert(eyeflow_mobile_short_debt_seconds(h) == debt); // STOP is not proof of rest.
    eyeflow_mobile_destroy(h);
    std::cout << "Corrected Android mobile C ABI contract passed\n";
}
