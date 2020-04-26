use jni::JNIEnv;
use jni::objects::{JClass, JObject, JString};
use jni::sys::{jboolean, jint, jlong, jobject, jstring};

use std::boxed::Box;
use std::ffi::c_void;
use std::sync::{Mutex, Arc, MutexGuard};
use std::collections::{LinkedList, HashMap};
use std::ptr::null;
use std::borrow::Borrow;
use dns_lookup::{AddrInfoHints, AddrInfo};

mod packet;
mod queue;

pub struct Manager {
    pub boxed_manager: Box<queue::Manager>,
}

impl Manager {
    pub fn new() -> Manager {
        boxed_manager: Box::new()
    }

    pub fn create(
        &mut self,
        queue_buffer_capacity: usize,
        packet_interval: i64
    ) -> *const queue::Manager {
        let lock = self.boxed_manager.lock;
        if lock.is_none() {
           return null();
        }
        let data: MutexGuard<Box<queue::Manager>> = lock.unwrap();
        *data.list = LinkedList::new();
        *data.queue_buffer_capacity = queue_buffer_capacity;
        *data.packet_interval = packet_interval;
        *data.shutting_down = false;
        *data.queues = HashMap::new();

        Box::into_raw(*data)
    }

    unsafe fn destroy(
        &mut self,
        manager_raw: *mut queue::Manager
    ) {
        drop(Box::from_raw(manager_raw));
    }

    unsafe fn get_remaining_capacity(
        &mut self,
        manager_raw: *mut queue::Manager,
        key: u64
    ) -> usize {
        let boxed_manager_mutex: Mutex<Box<queue::Manager>> = Mutex::new(Box::from_raw(manager_raw));
        let lock = boxed_manager_mutex.try_lock();

        if !lock.is_none() {
            let data = lock.unwrap();
            let queue = *data.queues[key];

            if queue.is_none() {
                queue.queue_buffer_capacity as usize
            } else {
                queue.buffer_capacity - queue.buffer_size
            }
        } else {
            0
        }
    }

    fn resolve_address(
        address: &str,
        port: i32
    ) -> Vec<AddrInfo> {
        let hints = dns_lookup::AddrInfoHints {
            socktype: dns_lookup::SockType::DGram.into(),
            protocol: dns_lookup::Protocol::UDP.into(),
            address: 0,
            flags: 0 | 2
        };

        dns_lookup::getaddrinfo(Some(address), Some(&(port.to_string())), Some(hints))
            .unwrap().collect::<std::io::Result<Vec<_>>>().unwrap()
    }
}

pub static MANAGER: Manager = Manager::new();

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_create(
    jni: JNIEnv,
    me: JObject,
    queue_buffer_capacity: jint,
    packet_interval: jlong,
) -> jlong {

}

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