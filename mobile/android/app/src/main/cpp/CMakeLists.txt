cmake_minimum_required(VERSION 3.22.1)
project(eyeflow_jni LANGUAGES CXX)
add_library(eyeflow_core SHARED IMPORTED)
set_target_properties(eyeflow_core PROPERTIES IMPORTED_SONAME "libeyeflow_core.so" IMPORTED_LOCATION "${CMAKE_CURRENT_SOURCE_DIR}/../jniLibs/${ANDROID_ABI}/libeyeflow_core.so")
add_library(eyeflow_jni SHARED eyeflow_jni.cpp)
target_link_libraries(eyeflow_jni PRIVATE eyeflow_core)
target_include_directories(eyeflow_jni PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/../../../../../../crates/eyeflow-core/include")
# Match Rust's 16 KiB ELF segment alignment for Android 15+ 16 KiB devices.
target_link_options(eyeflow_jni PRIVATE "-Wl,-z,max-page-size=16384")
