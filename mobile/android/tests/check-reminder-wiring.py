#!/usr/bin/env python3
"""Small source-wiring guard, not an Android compilation or runtime test.

Behavior is exercised by the production Kotlin policy tests. This checks that
the Android service actually uses that policy at the side-effect boundaries.
"""
from pathlib import Path
import re
import xml.etree.ElementTree as ET


ANDROID = Path(__file__).resolve().parents[1]
SOURCE = (ANDROID / "app/src/main/java/com/eyeflow/mobile/ReminderService.kt").read_text()


def function(name):
    """Select one named service member, avoiding whole-file order assertions."""
    members = list(re.finditer(r"^    (?:private |override )?fun (\w+)\(", SOURCE, re.M))
    for index, member in enumerate(members):
        if member.group(1) == name:
            end = members[index + 1].start() if index + 1 < len(members) else len(SOURCE)
            text = SOURCE[member.start():end]
            # Ignore comment/formatting differences, but retain string contents.
            tokens = re.findall(r'"(?:\\.|[^"\\])*"|//[^\n]*|/\*[\s\S]*?\*/|\S', text)
            return "".join(t for t in tokens if not t.startswith(("//", "/*")))
    raise AssertionError(f"Missing service member {name}")


policy = function("presentationPolicy")
assert "channel:String=channelFor(phase)" in policy, "Use the current phase's channel by default"
assert "getNotificationChannel(channel)" in policy, "Check the selected channel, not only the due channel"
assert "usagePermissionGranted=AppUsage.hasPermission(this)" in policy, "Recheck Usage Access at delivery time"
assert "PowerManager::class.java).isInteractive" in policy and "KeyguardManager::class.java).isKeyguardLocked" in policy

update = function("update")
assert "valpolicy=presentationPolicy(phase)" in update
assert "if(!policy.cardAllowed(coreCanDeliver))card.hide()" in update, "Apply DND and permission revocation to an already visible card"
assert "delivery.decide(policy.notificationAllowed(coreCanDeliver),reminderId,phase)" in update
assert "delivery.cancelled()" in update, "Withdraw visibility without clearing alert history"

show = function("showReminder")
build = show.index("valnotification=builder.build()")
post = show.index("notifications.notify(REST_ID,notification)")
card = show.index("card.show(")
assert build < post < card
assert "presentationPolicy(phase,channel).notificationAllowed(NativeCore.canDeliver(core))" in show[build:post], "Recheck immediately before notification posting"
assert "!prewarn&&!silent&&presentationPolicy(phase,channel).cardAllowed(NativeCore.canDeliver(core))" in show[post:card], "Recheck after notify and never reopen a card on silent restoration"
assert "setFullScreenIntent" not in show and "startActivity(" not in show
assert "handler.postDelayed(this, 1000)" in SOURCE, "Keep periodic live-policy checks independent of optional broadcasts"

manifest = ET.parse(ANDROID / "app/src/main/AndroidManifest.xml").getroot()
permissions = {node.attrib["{http://schemas.android.com/apk/res/android}name"] for node in manifest.findall("uses-permission")}
assert not permissions & {"android.permission.ACCESS_NOTIFICATION_POLICY", "android.permission.USE_FULL_SCREEN_INTENT"}, "Do not request extra DND access or full-screen delivery"
print("PASS: live reminder policy is wired to hide/cancel, current phase channel, and final notification/card gates (source check only)")
