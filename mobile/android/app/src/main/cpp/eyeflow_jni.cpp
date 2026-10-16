#include <jni.h>
#include <cstdint>
#include <vector>
#include "eyeflow.h"
static EyeFlowMobile* ptr(jlong handle) { return reinterpret_cast<EyeFlowMobile*>(static_cast<intptr_t>(handle)); }
extern "C" JNIEXPORT jlong JNICALL Java_com_eyeflow_mobile_NativeCore_create(JNIEnv*, jclass) { return reinterpret_cast<intptr_t>(eyeflow_mobile_create()); }
extern "C" JNIEXPORT void JNICALL Java_com_eyeflow_mobile_NativeCore_destroy(JNIEnv*, jclass, jlong p) { if(p) eyeflow_mobile_destroy(ptr(p)); }
extern "C" JNIEXPORT jint JNICALL Java_com_eyeflow_mobile_NativeCore_command(JNIEnv*, jclass, jlong p, jint c) { return p && c>=1 && c<=7 ? eyeflow_mobile_command(ptr(p), static_cast<uint32_t>(c)) : -1; }
extern "C" JNIEXPORT jint JNICALL Java_com_eyeflow_mobile_NativeCore_advance(JNIEnv*, jclass, jlong p, jlong ms, jint state, jboolean allowed) { return p && ms>=0 && state>=0 && state<=5 ? eyeflow_mobile_advance(ptr(p), static_cast<uint64_t>(ms), static_cast<uint32_t>(state), allowed ? 1 : 0) : -1; }
extern "C" JNIEXPORT jlong JNICALL Java_com_eyeflow_mobile_NativeCore_remaining(JNIEnv*, jclass, jlong p) { return p ? eyeflow_mobile_remaining_seconds(ptr(p)) : 0; }
extern "C" JNIEXPORT jint JNICALL Java_com_eyeflow_mobile_NativeCore_completed(JNIEnv*, jclass, jlong p) { return p ? eyeflow_mobile_completed_count(ptr(p)) : 0; }
extern "C" JNIEXPORT jint JNICALL Java_com_eyeflow_mobile_NativeCore_postponed(JNIEnv*, jclass, jlong p) { return p ? eyeflow_mobile_postponed_count(ptr(p)) : 0; }

extern "C" JNIEXPORT jbyteArray JNICALL Java_com_eyeflow_mobile_NativeCore_exportState(JNIEnv* env, jclass, jlong p) {
    if (!p) return nullptr;
    size_t size = eyeflow_mobile_export_state(ptr(p), nullptr, 0);
    if (!size || size > 1048576) return nullptr;
    std::vector<uint8_t> bytes(size);
    if (eyeflow_mobile_export_state(ptr(p), bytes.data(), size) != size) return nullptr;
    jbyteArray result = env->NewByteArray(static_cast<jsize>(size));
    if (result) env->SetByteArrayRegion(result, 0, static_cast<jsize>(size), reinterpret_cast<const jbyte*>(bytes.data()));
    return result;
}
extern "C" JNIEXPORT jboolean JNICALL Java_com_eyeflow_mobile_NativeCore_importState(JNIEnv* env, jclass, jlong p, jbyteArray data) {
    if (!p || !data) return JNI_FALSE;
    jsize size = env->GetArrayLength(data);
    if (size <= 0 || size > 1048576) return JNI_FALSE;
    std::vector<uint8_t> bytes(size);
    env->GetByteArrayRegion(data, 0, size, reinterpret_cast<jbyte*>(bytes.data()));
    return eyeflow_mobile_import_state(ptr(p), bytes.data(), size) == 0 ? JNI_TRUE : JNI_FALSE;
}

extern "C" JNIEXPORT jint JNICALL Java_com_eyeflow_mobile_NativeCore_natural(JNIEnv*, jclass, jlong p) { return p ? eyeflow_mobile_natural_count(ptr(p)) : 0; }
extern "C" JNIEXPORT jlong JNICALL Java_com_eyeflow_mobile_NativeCore_paused(JNIEnv*, jclass, jlong p) { return p ? eyeflow_mobile_paused_seconds(ptr(p)) : 0; }
extern "C" JNIEXPORT jlong JNICALL Java_com_eyeflow_mobile_NativeCore_shortDebt(JNIEnv*, jclass, jlong p) { return p ? eyeflow_mobile_short_debt_seconds(ptr(p)) : 0; }
extern "C" JNIEXPORT jlong JNICALL Java_com_eyeflow_mobile_NativeCore_longDebt(JNIEnv*, jclass, jlong p) { return p ? eyeflow_mobile_long_debt_seconds(ptr(p)) : 0; }
extern "C" JNIEXPORT jlong JNICALL Java_com_eyeflow_mobile_NativeCore_reminderId(JNIEnv*, jclass, jlong p) { return p ? eyeflow_mobile_reminder_id(ptr(p)) : 0; }
extern "C" JNIEXPORT jboolean JNICALL Java_com_eyeflow_mobile_NativeCore_canDeliver(JNIEnv*, jclass, jlong p) { return p && eyeflow_mobile_can_deliver(ptr(p)) ? JNI_TRUE : JNI_FALSE; }

extern "C" JNIEXPORT jboolean JNICALL Java_com_eyeflow_mobile_NativeCore_canPostpone(JNIEnv*, jclass, jlong p) { return p && eyeflow_mobile_can_postpone(ptr(p)) ? JNI_TRUE : JNI_FALSE; }
