//! 终端输出共享中心。
//!
//! PTY 读取线程通过 [`TerminalHub::publish`] 把输出写入按 `session_id` 分片的
//! 环形缓冲（容量 [`TERMINAL_BUFFER_CAPACITY`]），每帧携带单调递增的 `seq`；
//! 桌面端 Tauri 事件与 `/ws/terminal/:session_id` 的订阅者都从同一份数据扇出。
//!
//! 新订阅者调用 [`TerminalHub::subscribe`] 时：
//! - 不带 `since`：拿到整段缓冲的 `reset` 帧（清屏重绘）；
//! - 带 `since` 且缓冲中不存在空洞：拿到 `seq > since` 的增量 `chunk` 帧；
//! - 带 `since` 但对应帧已被淘汰：退回 `reset`，让客户端整屏重绘。

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::broadcast;

/// 每个会话的环形缓冲容量（字节）。
pub const TERMINAL_BUFFER_CAPACITY: usize = 256 * 1024;
/// 直播通道容量，够覆盖慢订阅者的瞬时积压。
const BROADCAST_CAPACITY: usize = 1024;

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalFrame {
    pub seq: u64,
    pub data: String,
}

/// 下发给订阅者的线格式。
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TerminalMessage {
    /// 客户端需先清屏，再写入 `data`。
    Reset { seq: u64, data: String },
    /// 客户端可直接追加 `data`。
    Chunk { seq: u64, data: String },
}

/// 订阅结果：先按序写完 `messages`，再消费 `receiver` 的直播帧。
#[derive(Debug)]
pub struct TerminalReplay {
    pub messages: Vec<TerminalMessage>,
    pub receiver: broadcast::Receiver<TerminalFrame>,
}

struct SessionState {
    frames: VecDeque<TerminalFrame>,
    /// 队首帧是否被截断过（截断后该 seq 的数据不再完整）。
    front_truncated: bool,
    buffered_bytes: usize,
    next_seq: u64,
    subscribers: usize,
    sender: broadcast::Sender<TerminalFrame>,
}

impl SessionState {
    fn new() -> Self {
        let (sender, _) = broadcast::channel(BROADCAST_CAPACITY);
        Self {
            frames: VecDeque::new(),
            front_truncated: false,
            buffered_bytes: 0,
            next_seq: 1,
            subscribers: 0,
            sender,
        }
    }

    fn head_seq(&self) -> u64 {
        self.next_seq.saturating_sub(1)
    }

    /// 缓冲中第一个数据完整的 seq。队首被截断时它不算完整。
    fn first_intact_seq(&self) -> Option<u64> {
        if self.front_truncated {
            self.frames.get(1).map(|frame| frame.seq)
        } else {
            self.frames.front().map(|frame| frame.seq)
        }
    }

    fn buffered_text(&self) -> String {
        self.frames.iter().map(|frame| frame.data.as_str()).collect()
    }

    /// 从队首丢弃数据直到不超过容量。截断点右移到 UTF-8 字符边界，
    /// 保证不会把一个多字节字符切成两半。
    fn trim(&mut self) {
        while self.buffered_bytes > TERMINAL_BUFFER_CAPACITY {
            let overflow = self.buffered_bytes - TERMINAL_BUFFER_CAPACITY;
            let Some(front) = self.frames.front_mut() else {
                break;
            };
            let front_len = front.data.len();
            if front_len <= overflow {
                self.buffered_bytes -= front_len;
                self.frames.pop_front();
                self.front_truncated = false;
                continue;
            }
            let mut cut = overflow;
            while cut < front_len && !front.data.is_char_boundary(cut) {
                cut += 1;
            }
            front.data = front.data[cut..].to_string();
            self.buffered_bytes -= cut;
            self.front_truncated = true;
        }
    }
}

#[derive(Default)]
pub struct TerminalHub {
    sessions: Mutex<HashMap<String, SessionState>>,
}

impl TerminalHub {
    /// 追加一帧输出并返回它的 `seq`。
    pub fn publish(&self, session_id: &str, data: &str) -> u64 {
        let mut sessions = self.sessions.lock().expect("terminal hub poisoned");
        let session = sessions
            .entry(session_id.to_string())
            .or_insert_with(SessionState::new);
        let seq = session.next_seq;
        session.next_seq += 1;
        let frame = TerminalFrame {
            seq,
            data: data.to_string(),
        };
        session.buffered_bytes += data.len();
        session.frames.push_back(frame.clone());
        session.trim();
        // 没有直播订阅者时 send 返回 Err，属正常情况。
        let _ = session.sender.send(frame);
        seq
    }

    /// 订阅会话输出。`since` 为客户端已完整收到的最后一个 `seq`。
    pub fn subscribe(&self, session_id: &str, since: Option<u64>) -> TerminalReplay {
        let mut sessions = self.sessions.lock().expect("terminal hub poisoned");
        let session = sessions
            .entry(session_id.to_string())
            .or_insert_with(SessionState::new);
        let receiver = session.sender.subscribe();
        let head = session.head_seq();
        let messages = if session.frames.is_empty() {
            Vec::new()
        } else {
            match since {
                Some(since) if since >= head => Vec::new(),
                Some(since)
                    if session
                        .first_intact_seq()
                        .is_some_and(|intact| intact <= since + 1) =>
                {
                    session
                        .frames
                        .iter()
                        .filter(|frame| frame.seq > since)
                        .map(|frame| TerminalMessage::Chunk {
                            seq: frame.seq,
                            data: frame.data.clone(),
                        })
                        .collect()
                }
                _ => vec![TerminalMessage::Reset {
                    seq: head,
                    data: session.buffered_text(),
                }],
            }
        };
        TerminalReplay { messages, receiver }
    }

    /// 登记一个订阅者，返回的 guard 析构时自动注销。
    pub fn register_subscriber_arc(hub: &Arc<TerminalHub>, session_id: &str) -> SubscriberGuard {
        {
            let mut sessions = hub.sessions.lock().expect("terminal hub poisoned");
            sessions
                .entry(session_id.to_string())
                .or_insert_with(SessionState::new)
                .subscribers += 1;
        }
        SubscriberGuard {
            hub: Arc::clone(hub),
            session_id: session_id.to_string(),
        }
    }

    /// 该会话当前是否有远端订阅者（写入类命令的授权前置条件）。
    pub fn has_subscriber(&self, session_id: &str) -> bool {
        let sessions = self.sessions.lock().expect("terminal hub poisoned");
        sessions
            .get(session_id)
            .map(|session| session.subscribers > 0)
            .unwrap_or(false)
    }

    /// 会话结束时丢弃全部状态。
    pub fn close(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().expect("terminal hub poisoned");
        sessions.remove(session_id);
    }

    fn release_subscriber(&self, session_id: &str) {
        let mut sessions = self.sessions.lock().expect("terminal hub poisoned");
        if let Some(session) = sessions.get_mut(session_id) {
            session.subscribers = session.subscribers.saturating_sub(1);
        }
    }
}

pub struct SubscriberGuard {
    hub: Arc<TerminalHub>,
    session_id: String,
}

impl Drop for SubscriberGuard {
    fn drop(&mut self) {
        self.hub.release_subscriber(&self.session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_assigns_monotonic_sequence_numbers() {
        let hub = TerminalHub::default();
        assert_eq!(hub.publish("s1", "a"), 1);
        assert_eq!(hub.publish("s1", "b"), 2);
        assert_eq!(hub.publish("s2", "a"), 1);
    }

    #[test]
    fn fresh_subscriber_receives_full_buffer_as_reset() {
        let hub = TerminalHub::default();
        hub.publish("s1", "hello ");
        hub.publish("s1", "world");
        let replay = hub.subscribe("s1", None);
        assert_eq!(
            replay.messages,
            vec![TerminalMessage::Reset {
                seq: 2,
                data: "hello world".to_string()
            }]
        );
    }

    #[test]
    fn fresh_session_without_output_yields_no_replay() {
        let hub = TerminalHub::default();
        assert!(hub.subscribe("s1", None).messages.is_empty());
    }

    #[test]
    fn subscriber_with_since_receives_only_newer_chunks() {
        let hub = TerminalHub::default();
        hub.publish("s1", "one");
        hub.publish("s1", "two");
        hub.publish("s1", "three");
        let replay = hub.subscribe("s1", Some(1));
        assert_eq!(
            replay.messages,
            vec![
                TerminalMessage::Chunk {
                    seq: 2,
                    data: "two".to_string()
                },
                TerminalMessage::Chunk {
                    seq: 3,
                    data: "three".to_string()
                },
            ]
        );
    }

    #[test]
    fn since_beyond_head_yields_no_replay() {
        let hub = TerminalHub::default();
        hub.publish("s1", "one");
        let replay = hub.subscribe("s1", Some(9));
        assert!(replay.messages.is_empty());
    }

    // 客户端已收到 seq 1，但 seq 2 已被完全淘汰 —— 存在空洞，必须整屏重绘。
    #[test]
    fn since_older_than_evicted_frames_falls_back_to_reset() {
        let hub = TerminalHub::default();
        let filler = "x".repeat(TERMINAL_BUFFER_CAPACITY);
        hub.publish("s1", "a");
        hub.publish("s1", "b");
        hub.publish("s1", &filler);
        let replay = hub.subscribe("s1", Some(1));
        assert_eq!(
            replay.messages,
            vec![TerminalMessage::Reset {
                seq: 3,
                data: filler
            }]
        );
    }

    // seq 1 被部分截断，但 since=1 的客户端已完整收过 seq 1，仍可增量续传。
    #[test]
    fn since_at_truncation_boundary_still_streams_chunks() {
        let hub = TerminalHub::default();
        hub.publish("s1", &"x".repeat(TERMINAL_BUFFER_CAPACITY));
        hub.publish("s1", "tail");
        let replay = hub.subscribe("s1", Some(1));
        assert_eq!(
            replay.messages,
            vec![TerminalMessage::Chunk {
                seq: 2,
                data: "tail".to_string()
            }]
        );
    }

    // 同一场景下，从未收过任何帧的客户端只能拿到被截断后的残余缓冲。
    #[test]
    fn fresh_subscriber_after_truncation_receives_trimmed_buffer() {
        let hub = TerminalHub::default();
        hub.publish("s1", &"x".repeat(TERMINAL_BUFFER_CAPACITY));
        hub.publish("s1", "tail");
        let replay = hub.subscribe("s1", None);
        let expected = format!("{}tail", "x".repeat(TERMINAL_BUFFER_CAPACITY - 4));
        assert_eq!(expected.len(), TERMINAL_BUFFER_CAPACITY);
        assert_eq!(
            replay.messages,
            vec![TerminalMessage::Reset {
                seq: 2,
                data: expected
            }]
        );
    }

    #[test]
    fn trim_never_splits_a_multibyte_character() {
        let hub = TerminalHub::default();
        // 「中」占 3 字节。262144 / 3 = 87381，故 wide 为 262143 字节；
        // 追加 2 字节后总量 262146，溢出 2 字节。截断点落在第一个「中」内部，
        // 必须右移到 3 才是字符边界，因此实际丢弃 3 字节（整个首字符）。
        let char_count = TERMINAL_BUFFER_CAPACITY / 3;
        let wide = "中".repeat(char_count);
        assert_eq!(wide.len(), TERMINAL_BUFFER_CAPACITY - 1);
        hub.publish("s1", &wide);
        hub.publish("s1", "ab");
        let replay = hub.subscribe("s1", None);
        let expected = format!("{}ab", "中".repeat(char_count - 1));
        assert_eq!(expected.len(), TERMINAL_BUFFER_CAPACITY - 2);
        assert_eq!(
            replay.messages,
            vec![TerminalMessage::Reset {
                seq: 2,
                data: expected
            }]
        );
    }

    #[tokio::test]
    async fn all_subscribers_receive_live_frames() {
        let hub = TerminalHub::default();
        let mut first = hub.subscribe("s1", None).receiver;
        let mut second = hub.subscribe("s1", None).receiver;
        hub.publish("s1", "live");
        assert_eq!(
            first.recv().await.unwrap(),
            TerminalFrame {
                seq: 1,
                data: "live".to_string()
            }
        );
        assert_eq!(
            second.recv().await.unwrap(),
            TerminalFrame {
                seq: 1,
                data: "live".to_string()
            }
        );
    }

    #[test]
    fn subscriber_guard_registers_and_unregisters() {
        let hub = Arc::new(TerminalHub::default());
        assert!(!hub.has_subscriber("s1"));
        let guard = TerminalHub::register_subscriber_arc(&hub, "s1");
        assert!(hub.has_subscriber("s1"));
        let second = TerminalHub::register_subscriber_arc(&hub, "s1");
        drop(guard);
        assert!(hub.has_subscriber("s1"));
        drop(second);
        assert!(!hub.has_subscriber("s1"));
    }

    #[test]
    fn close_drops_session_state() {
        let hub = TerminalHub::default();
        hub.publish("s1", "data");
        hub.close("s1");
        assert!(hub.subscribe("s1", None).messages.is_empty());
        assert_eq!(hub.publish("s1", "again"), 1);
    }

    #[test]
    fn messages_serialize_with_lowercase_type_tag() {
        let reset = serde_json::to_string(&TerminalMessage::Reset {
            seq: 7,
            data: "hi".to_string(),
        })
        .unwrap();
        assert_eq!(reset, r#"{"type":"reset","seq":7,"data":"hi"}"#);
        let chunk = serde_json::to_string(&TerminalMessage::Chunk {
            seq: 8,
            data: "ho".to_string(),
        })
        .unwrap();
        assert_eq!(chunk, r#"{"type":"chunk","seq":8,"data":"ho"}"#);
    }
}
