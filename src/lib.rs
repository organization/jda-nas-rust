#[macro_use]
extern crate lazy_static;

use std::cell::RefCell;
use std::ffi::c_void;
use std::net::SocketAddr;
use std::ops::Add;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use dns_lookup::AddrInfo;
use jni::JNIEnv;
use jni::objects::{JByteBuffer, JObject, JString};
use jni::sys::{jboolean, jint, jlong, jobject, jstring};
use tokio::net::UdpSocket;

mod packet;
mod queue;
mod utils;

pub struct Manager<'a> {
    pub boxed_manager_mutex: Arc<Mutex<Box<queue::Manager<'a>>>>,
}

thread_local! {
    // TODO: proper error handling
    static RUNTIME: RefCell<tokio::runtime::Runtime> = RefCell::new(tokio::runtime::Runtime::new().unwrap());
}

impl<'a> Manager<'a> {
    pub fn new(queue_buffer_capacity: usize, packet_interval: u128) -> Manager<'a> {
        Manager {
            boxed_manager_mutex: Arc::new(Mutex::new(Box::new(queue::Manager::new(
                queue_buffer_capacity,
                packet_interval,
            )))),
        }
    }

    fn destroy(&self) {
        if let Ok(mut manager) = self.boxed_manager_mutex.try_lock() {
            manager.shutting_down = true;
        }
    }

    fn get_remaining_capacity(&self, key: u64) -> usize {
        if let Ok(manager) = self.boxed_manager_mutex.try_lock() {
            let queue = manager.queues.get(&key);
            if let Some(queue) = queue {
                queue.buffer.capacity - queue.buffer.size
            } else {
                manager.queue_buffer_capacity as usize
            }
        } else {
            0
        }
    }

    fn resolve_address(&self, address: &str, port: i32) -> Vec<AddrInfo> {
        let hints = dns_lookup::AddrInfoHints {
            socktype: dns_lookup::SockType::DGram.into(),
            protocol: dns_lookup::Protocol::UDP.into(),
            address: 0,
            flags: 0 | 2,
        };

        dns_lookup::getaddrinfo(Some(address), Some(&(port.to_string())), Some(hints))
            .into_iter()
            .flatten()
            .flatten()
            .collect()
    }

    fn queue_packet(
        &self,
        key: u64,
        address: String,
        port: i32,
        data: Vec<u8>,
        data_length: usize,
        explicit_socket: Option<&'a mut UdpSocket>,
    ) -> bool {
        if let Ok(mut manager) = self.boxed_manager_mutex.try_lock() {
            let key_exists = manager.queues.contains_key(&key);
            if !key_exists {
                let addresses = self.resolve_address(&address, port);
                if addresses.is_empty() {
                    return false;
                }
                let capacity = manager.queue_buffer_capacity;
                manager.queues.insert(
                    key,
                    queue::Item {
                        next_due_time: 0,
                        packet_buffer: vec![],
                        buffer: queue::Buffer {
                            index: 0,
                            size: 0,
                            capacity,
                        },
                        address: addresses,
                        explicit_socket,
                    },
                );
            }
            if let Some(item) = manager.queues.get_mut(&key) {
                if item.buffer.size >= item.buffer.capacity {
                    false
                } else {
                    let next_index = (item.buffer.index + item.buffer.size) % item.buffer.capacity;

                    item.packet_buffer.insert(next_index, packet::Queued {
                        data,
                        data_length,
                    });
                    item.buffer.size += 1;
                    true
                }
            } else {
                false
            }
        } else {
            false
        }
    }

    fn queue_delete(&self, key: u64) -> bool {
        if let Ok(mut manager) = self.boxed_manager_mutex.try_lock() {
            if let Some(item) = manager.queues.get_mut(&key) {
                while item.buffer.size > 0 {
                    let index = item.buffer.index;
                    item.buffer.index = (item.buffer.index + 1) % item.buffer.capacity;
                    item.buffer.size -= 1;
                    item.packet_buffer.remove(index);
                }
                true
            } else {
                false
            }
        } else {
            false
        }
    }

    fn get_target_time(&self, current_time: u128) -> u128 {
        if let Ok(manager) = self.boxed_manager_mutex.try_lock() {
            if let Some(item) = manager.queue_linked.front() {
                item.next_due_time
            } else {
                current_time + manager.packet_interval
            }
        } else {
            current_time
        }
    }

    fn queue_pop_packet(&self, item: &mut queue::Item) -> packet::Unsent {
        let index = item.buffer.index;

        item.buffer.index = (item.buffer.index + 1) % item.buffer.capacity;
        item.buffer.size -= 1;

        let packet = item.packet_buffer.remove(index);

        let unsent_packet = packet::Unsent {
            packet,
            address: item.address.clone(),
        };

        return unsent_packet;
    }

    fn process_next(&self, mut current_time: u128) -> (Option<packet::Unsent>, u128) {
        if let Ok(mut manager) = self.boxed_manager_mutex.try_lock() {
            if let Some(mut item) = manager.queue_linked.pop_front() {
                if item.next_due_time == 0 {
                    item.next_due_time = current_time;
                } else if item.next_due_time - current_time >= 1500000 {
                    return (None, item.next_due_time);
                }
                let packet = self.queue_pop_packet(&mut item);
                current_time = utils::timing_get_nano_secs();
                if current_time - item.next_due_time >= 2 * (manager.packet_interval) {
                    item.next_due_time = current_time + manager.packet_interval;
                } else {
                    item.next_due_time = manager.packet_interval;
                }
                manager.queue_linked.push_back(item);
                (Some(packet), self.get_target_time(current_time))
            } else {
                (None, current_time + manager.packet_interval)
            }
        } else {
            (None, 0)
        }
    }

    async fn dispatch_packet(&self, socket_vx: &mut UdpSocket, unsent_packet: &packet::Unsent) {
        let remote_addr = unsent_packet.address.get(0).unwrap().sockaddr;
        socket_vx.connect(remote_addr).await;
        socket_vx.send(unsent_packet.packet.data.as_ref()).await;
    }

    async fn process_with_socket(&self) {
        if let Ok(manager) = self.boxed_manager_mutex.try_lock() {
            loop {
                if manager.shutting_down {
                    break;
                }

                let mut current_time = utils::timing_get_nano_secs();

                let (packet_to_send, target_time) = self.process_next(current_time);

                if let Some(packet_to_send) = packet_to_send {
                    let local_addr: SocketAddr =
                        if packet_to_send.address.get(0).unwrap().sockaddr.is_ipv4() {
                            "0.0.0.0:0"
                        } else {
                            "[::]:0"
                        }
                        .parse()
                        .unwrap();
                    let mut socket_vx = match UdpSocket::bind(local_addr).await {
                        Ok(n) => n,
                        Err(_) => {
                            eprintln!("Can't bind!");
                            break;
                        }
                    };
                    current_time = utils::timing_get_nano_secs();
                }

                let wait_time = target_time - current_time;

                if wait_time >= 1500000u128 {
                    thread::sleep(Duration::from_nanos(wait_time as u64))
                }
            }
        }
    }

    fn process(&self) {
        self.process_with_socket();
    }
}

lazy_static! {
    pub static ref INDEX: Mutex<i64> = Mutex::new(i64::MIN);
    pub static ref MANAGERS: Mutex<std::collections::HashMap<i64, Box<Manager<'static>>>> = Mutex::new(Default::default());
}


#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_create(
    jni: JNIEnv,
    me: JObject,
    queue_buffer_capacity: jint,
    packet_interval: jlong,
) -> jlong {
    let refcell_manager: Box<Manager> = Box::new(Manager::new(
        queue_buffer_capacity as usize,
        packet_interval as u128,
    ));

    let index = INDEX.lock().unwrap().clone();
    MANAGERS.lock().unwrap().insert(index, refcell_manager);
    *INDEX.lock().unwrap() += 1;
    return index
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_destroy(
    jni: JNIEnv,
    me: JObject,
    instance: jlong,
) {
    let manager_mutex = MANAGERS.lock().unwrap();
    let manager = manager_mutex.get(&instance);

    if let Some(manager) = manager.as_ref() {
        manager.destroy();
    }
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_getRemainingCapacity(
    jni: JNIEnv,
    me: JObject,
    instance: jlong,
    key: jlong,
) -> jint {
    let manager_mutex = MANAGERS.lock().unwrap();
    let manager = manager_mutex.get(&instance);

    if let Some(manager) = manager.as_ref() {
        manager.get_remaining_capacity(key as u64) as i32
    } else {
        0
    }
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_queuePacket(
    jni: JNIEnv,
    me: JObject,
    instance: jlong,
    key: jlong,
    address_string: JString,
    port: jint,
    data_buffer: JByteBuffer,
    data_length: jint,
) -> jboolean {
    let address = jni
        .get_string(address_string)
        .expect("Couldn't get java string!")
        .into();
    let bytes = jni
        .get_direct_buffer_address(data_buffer)
        .expect("Couldn't get java ByteBuffer!")
        .to_vec();

    let manager_mutex = MANAGERS.lock().unwrap();
    let manager = manager_mutex.get(&instance);

    if let Some(manager) = manager.as_ref() {
        if manager.queue_packet(key as u64, address, port, bytes, data_length as usize, None) {
            jni::sys::JNI_TRUE
        } else {
            jni::sys::JNI_FALSE
        }
    } else {
        jni::sys::JNI_FALSE
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
    /*let boxed_manager: Box<Manager> = unsafe { Box::from_raw(instance as *mut Manager) };

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
        jni::sys::JNI_TRUE
    } else {
        jni::sys::JNI_FALSE
    }*/
    unimplemented!("queuePacketWithSocket is not implemented!")
}

#[no_mangle]
pub extern "system" fn Java_com_sedmelluq_discord_lavaplayer_udpqueue_natives_UdpQueueManagerLibrary_deleteQueue(
    jni: JNIEnv,
    me: JObject,
    instance: jlong,
    key: jlong,
) -> jboolean {
    let manager_mutex = MANAGERS.lock().unwrap();
    let manager = manager_mutex.get(&instance);

    if let Some(manager) = manager.as_ref() {
        if manager.queue_delete(key as u64) {
            jni::sys::JNI_TRUE
        } else {
            jni::sys::JNI_FALSE
        }
    } else {
        jni::sys::JNI_FALSE
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
    let manager_mutex = MANAGERS.lock().unwrap();
    let manager = manager_mutex.get(&instance);

    if let Some(manager) = manager.as_ref() {
        let task = manager.process_with_socket();
        RUNTIME.with(move |rt| rt.borrow_mut().block_on(task));
    }
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
