use jni::JNIEnv;
use jni::objects::{JClass, JObject, JString};
use jni::sys::{jboolean, jint, jlong, jobject, jstring};

use std::ffi::c_void;

mod packet;
mod queue;

struct Manager {
    manager: Box<queue::Manager>
}

impl Manager {
    
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_create(
    jni: JNIEnv,
    me: JObject,
    queue_buffer_capacity: jint,
    packet_interval: jlong,
) -> jlong {}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_destroy(
    jni: JNIEnv,
    me: JObject,
    instance: jlong,
) {}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_getRemainingCapacity(
    jni: JNIEnv,
    me: JObject,
    instance: jlong,
    key: jlong,
) -> jint {}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_queuePacket(
    jni: JNIEnv,
    me: JObject,
    instance: jlong,
    key: jlong,
    address_string: jstring,
    port: jint,
    data_buffer: jobject,
    data_length: jint,
) -> jboolean {}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_queuePacketWithSocket(
    jni: JNIEnv,
    me: JObject,
    instance: jlong,
    key: jlong,
    address_string: jstring,
    port: jint,
    data_buffer: jobject,
    data_length: jint,
    socket_handle: jlong,
) -> jboolean {}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_deleteQueue(
    jni: JNIEnv,
    me: JObject,
    instance: jlong,
    key: jlong,
) -> jboolean {}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_processWithSocket(
    jni: JNIEnv,
    me: JObject,
    instance: jlong,
    socket_v4: jlong,
    socket_v6: jlong,
) {}

fn waiting_iterate_callback(
    class_tag: jlong,
    size: jlong,
    tag_ptr: *mut jlong,
    length: jint,
    user_data: *mut c_void,
) -> jint {}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_pauseDemo(
    jni: JNIEnv,
    me: JObject,
    length: jint,
) {}