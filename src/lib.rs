use std::{ptr, thread};
use std::borrow::Borrow;
use std::boxed::Box;
use std::cell::RefCell;
use std::collections::{HashMap, LinkedList};
use std::ffi::c_void;
use std::net::SocketAddr;
use std::ops::Deref;
use std::ptr::null;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use dns_lookup::{AddrInfo, AddrInfoHints};
use jni::{errors, JavaVM, JNIEnv, sys};
use jni::errors::jni_error_code_to_result;
use jni::objects::{JByteBuffer, JClass, JObject, JString};
use jni::sys::{jboolean, jint, jlong, jobject, jstring};
use tokio::net::UdpSocket;

use jvmti_sys::jvmtiCapabilities;

mod packet;
mod queue;
mod utils;

pub struct Manager {
    pub boxed_manager_mutex: Arc<Mutex<RefCell<queue::Manager>>>,
}

impl Manager {
    pub fn new(
        queue_buffer_capacity: usize,
        packet_interval: u128,
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
            let mut manager: MutexGuard<RefCell<queue::Manager>> = lock.unwrap();
            manager.get_mut().shutting_down = true;
        }
    }

    fn get_remaining_capacity(
        &self,
        key: u64,
    ) -> usize {
        let lock = self.boxed_manager_mutex.try_lock();

        if !lock.is_none() {
            let mut manager: MutexGuard<RefCell<queue::Manager>> = lock.unwrap();
            let queue = manager.get_mut().queues[key];

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
        explicit_socket: Option<UdpSocket>,
    ) -> bool {
        let lock = self.boxed_manager_mutex.try_lock();

        if !lock.is_none() {
            let mut manager: MutexGuard<RefCell<queue::Manager>> = lock.unwrap();

            let exist: bool = manager.get_mut().queues.contains_key(key.borrow());

            if !exist {
                let address = self.resolve_address(
                    &address,
                    port,
                );

                if address.empty() {
                    false
                }

                manager.get_mut().queues.insert(
                    key,
                    queue::Item {
                        next_due_time: 0,
                        packet_buffer: vec!(),
                        buffer: queue::Buffer {
                            index: 0,
                            size: 0,
                            capacity: manager.get_mut().queue_buffer_capacity,
                        },
                        address,
                        explicit_socket,
                    },
                );
                manager.get_mut().queue_linked.push_front(inserted_item);
            }

            let item: Option<&queue::Item> = manager.get_mut().queues.get(key.borrow());

            if item.is_none() {
                false
            } else {
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

    fn queue_delete(
        &self,
        key: u64,
    ) -> bool {
        let lock = self.boxed_manager_mutex.try_lock();

        if !lock.is_none() {
            let mut manager: MutexGuard<RefCell<queue::Manager>> = lock.unwrap();
            let item: Option<&queue::Item> = manager.get_mut().queues.get(key.borrow());

            if item.is_none() {
                false
            } else {
                let unwrapped_item = item.unwrap();

                if unwrapped_item.buffer.size <= 0 {
                    false
                }

                while unwrapped_item.buffer.size > 0 {
                    let index = unwrapped_item.buffer.index;

                    &unwrapped_item.buffer.index = ((unwrapped_item.buffer.index + 1) % unwrapped_item.buffer.capacity).borrow();
                    &unwrapped_item.buffer.size -= 1;

                    (&unwrapped_item.packet_buffer).remove(index);
                }

                true
            }
        } else {
            false
        }
    }

    fn get_target_time(
        &self,
        current_time: u128,
    ) -> u128 {
        let lock = self.boxed_manager_mutex.try_lock();

        if !lock.is_none() {
            let mut manager: MutexGuard<RefCell<queue::Manager>> = lock.unwrap();
            let item: Option<&queue::Item> = *manager.get_mut().queue_linked.front();

            if item.is_none() {
                current_time + manager.get_mut().packet_interval
            } else {
                item.unwrap().next_due_time
            }
        }
    }

    fn queue_pop_packet(
        item: &queue::Item
    ) -> packet::Unsent {
        let index = item.buffer.index;
        let _packet = *item.packet_buffer[index];

        &item.buffer.index = ((item.buffer.index + 1) % item.buffer.capacity).borrow();
        &item.buffer.size -= 1;

        let unsent_packet = packet::Unsent {
            packet: _packet,
            address: item.address,
            explicit_socket: item.explicit_socket,
        }

            (&item.packet_buffer).remove(index);

        return unsent_packet;
    }

    fn process_next(
        &self,
        mut current_time: u128,
    ) -> (Option<packet::Unsent>, u128) {
        let lock = self.boxed_manager_mutex.try_lock();

        if !lock.is_none() {
            let mut manager: MutexGuard<RefCell<queue::Manager>> = lock.unwrap();
            let item: Option<&queue::Item> = *manager.get_mut().queue_linked.front();

            if item.is_none() {
                current_time + manager.get_mut().packet_interval
            }

            let unwrapped_item = item.unwrap();

            if unwrapped_item.next_due_time == 0 {
                &unwrapped_item.next_due_time = &current_time;
            } else if unwrapped_item.next_due_time - current_time >= 1500000u128 {
                unwrapped_item.next_due_time
            }

            let unsent_packet: packet::Unsent = self.queue_pop_packet(item);

            manager.get_mut().queue_linked.push_back(manager.get_mut().queue_linked.pop_front().unwrap());

            current_time = utils::timing_get_nano_secs();

            if current_time - unwrapped_item.next_due_time >= 2 * (manager.get_mut().packet_interval) {
                &unwrapped_item.next_due_time = current_time + (*manager.get_mut().packet_interval);
            } else {
                &unwrapped_item.next_due_time = *manager.get_mut().packet_interval;
            }

            (Some(unsent_packet), self.get_target_time(current_time))
        } else {
            (None, 0);
        }
    }

    fn dispatch_packet(
        mut socket_vx: UdpSocket,
        unsent_packet: &packet::Unsent,
    ) {
        let remote_addr = unsent_packet.address.get(0).unwrap().sockaddr;
        socket_vx.connect(remote_addr).await?;
        socket_vx.send(unsent_packet.packet.data.as_ref()).await?;
    }

    fn process_with_socket(
        &self
    ) {
        let lock = self.boxed_manager_mutex.try_lock();

        if !lock.is_none() {
            let mut manager: MutexGuard<RefCell<queue::Manager>> = lock.unwrap();

            loop {
                if manager.get_mut().shutting_down {
                    break;
                }

                let mut current_time = utils::timing_get_nano_secs();

                let (packet_to_send, target_time) = self.process_next(current_time);

                drop(*manager);

                if !packet_to_send.is_none() {
                    let unwrapped_pts = packet_to_send.unwrap();

                    if unwrapped_pts.explicit_socket.is_none() {
                        let local_addr: SocketAddr = if remote_addr.is_ipv4() {
                            "0.0.0.0:0"
                        } else {
                            "[::]:0"
                        }.parse()?;
                        let socket_vx = UdpSocket::bind(local_addr).await?;
                        self.dispatch_packet(socket_vx, *unwrapped_pts);
                    } else {
                        let unwrapped_socket = unwrapped_pts.explicit_socket.unwrap();
                        self.dispatch_packet(unwrapped_socket, *unwrapped_pts);
                    }

                    current_time = utils::timing_get_nano_secs();
                }

                let wait_time = target_time - current_time;

                if wait_time >= 1500000u128 {
                    thread::sleep(
                        Duration::from_nanos(wait_time as u64)
                    )
                }
            }
        }
    }

    fn process(
        &self
    ) {
        self.process_with_socket();
    }
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_create(
    jni: JNIEnv,
    me: JObject,
    queue_buffer_capacity: jint,
    packet_interval: jlong,
) -> jlong {
    let boxed_manager: Box<Manager> = Box::new(
        Manager::new(
            queue_buffer_capacity as usize,
            packet_interval as u128,
        )
    );

    Box::into_raw(boxed_manager) as jlong
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_destroy(
    jni: JNIEnv,
    me: JObject,
    instance: jlong,
) {
    let boxed_manager: Box<Manager> = unsafe { Box::from_raw(*instance) };
    boxed_manager.destroy();
    drop(boxed_manager);
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_getRemainingCapacity(
    jni: JNIEnv,
    me: JObject,
    instance: jlong,
    key: jlong,
) -> jint {
    let boxed_manager: Box<Manager> = unsafe { Box::from_raw(*instance) };
    boxed_manager.get_remaining_capacity(key as u64) as i32
}

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
) -> jboolean {
    let boxed_manager: Box<Manager> = unsafe { Box::from_raw(*instance) };

    let address: String = JString::from(address_string).into();
    let bytes = JByteBuffer::from(data_buffer);

    return if boxed_manager.queue_packet(key as u64, address, port, bytes.into(), data_length as usize, None) {
        JNI_TRUE
    } else {
        JNI_FALSE
    }
}

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
) -> jboolean {
    // It does not work. Also, this method isn't used in jda-nas.
    let boxed_manager: Box<Manager> = unsafe { Box::from_raw(*instance) };

    let address: String = JString::from(address_string).into();
    let bytes = JByteBuffer::from(data_buffer);

    return if boxed_manager.queue_packet(
        key as u64,
        address,
        port,
        bytes.into(),
        data_length as usize,
        Some(unsafe { Box::from_raw(socket_handle as *mut UdpSocket) }.as_sock()),
    ) {
        JNI_TRUE
    } else {
        JNI_FALSE
    }
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_deleteQueue(
    jni: JNIEnv,
    me: JObject,
    instance: jlong,
    key: jlong,
) -> jboolean {
    let boxed_manager: Box<Manager> = unsafe { Box::from_raw(*instance) };

    if boxed_manager.queue_delete(key as u64) {
        JNI_TRUE
    } else {
        JNI_FALSE
    }
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_processWithSocket(
    jni: JNIEnv,
    me: JObject,
    instance: jlong,
    socket_v4: jlong,
    socket_v6: jlong,
) {
    let boxed_manager: Box<Manager> = unsafe { Box::from_raw(*instance) };

    boxed_manager.process_with_socket();
}

fn waiting_iterate_callback(
    class_tag: jlong,
    size: jlong,
    tag_ptr: *mut jlong,
    length: jint,
    user_data: *mut c_void,
) -> jint {
    unimplemented!("waiting_iterate_callback is not implemented!")
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_pauseDemo(
    jni: JNIEnv,
    me: JObject,
    length: jint,
) {
    unimplemented!("pauseDemo is not implemented!")
}