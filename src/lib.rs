use std::borrow::Borrow;
use std::boxed::Box;
use std::cell::RefCell;
use std::collections::{HashMap, LinkedList};
use std::ffi::c_void;
use std::ptr::null;
use std::sync::{Arc, Mutex, MutexGuard};

use dns_lookup::{AddrInfo, AddrInfoHints};
use jni::JNIEnv;
use jni::objects::{JClass, JObject, JString};
use jni::sys::{jboolean, jint, jlong, jobject, jstring};
use tokio::net::UdpSocket;

mod packet;
mod queue;

pub struct Manager {
    pub boxed_manager_mutex: Arc<Mutex<RefCell<queue::Manager>>>,
}

impl Manager {
    pub fn new(
        queue_buffer_capacity: usize,
        packet_interval: i64,
    ) -> Manager {
        Manager {
            boxed_manager_mutex: Arc::new(
                Mutex::new(
                    RefCell::new(
                        queue::Manager::new(
                            queue_buffer_capacity,
                            packet_interval,
                        )
                    )
                )
            )
        }
    }

    fn destroy(
        &self
    ) {
        let lock = self.boxed_manager_mutex.try_lock();
        if !lock.is_none() {
            let manager: MutexGuard<RefCell<queue::Manager>> = lock.unwrap();
            *manager.shutting_down = true;
        }
    }

    fn get_remaining_capacity(
        &self,
        key: u64,
    ) -> usize {
        let lock = self.boxed_manager_mutex.try_lock();

        if !lock.is_none() {
            let manager: MutexGuard<RefCell<queue::Manager>> = lock.unwrap();
            let queue = *manager.queues[key];

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

    fn queue_packet(
        &self,
        key: u64,
        address: String,
        port: i32,
        data: Vec<u8>,
        data_length: usize,
        explicit_socket: UdpSocket,
    ) -> bool {
        let lock = self.boxed_manager_mutex.try_lock();

        if !lock.is_none() {
            let manager: MutexGuard<RefCell<queue::Manager>> = lock.unwrap();

            let exist: bool = *manager.queues.contains_key(key);

            if !exist {
                *manager.queues.insert(
                    key,
                    queue::Item {
                        next_due_time: 0,
                        packet_buffer: vec!(),
                        buffer: queue::Buffer {
                            index: 0,
                            size: 0,
                            capacity: *manager.queue_buffer_capacity,
                        },
                        address: self.resolve_address(
                            &address,
                            port,
                        ),
                        explicit_socket,
                    },
                );
            }

            let item: Option<&queue::Item> = *manager.queues.get(key);

            if item.is_none() {
                false
            } else {
                *manager.push_front(inserted_item);
                let unwrapped_item = item.unwrap();

                if unwrapped_item.buffer.size >= unwrapped_item.buffer.capacity {
                    false
                }

                let next_index = (unwrapped_item.buffer.index + unwrapped_item.buffer.size) % unwrapped_item.buffer.capacity;

                &unwrapped_item.packet_buffer[next_index].data = &data;
                &unwrapped_item.packet_buffer[next_index].data_length = &data_length;
                &unwrapped_item.buffer.size += 1;

                true
            }
        } else {
            false
        }
    }
}

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