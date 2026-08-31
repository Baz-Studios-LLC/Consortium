// Who runs, when, and never twice.
//
// The router decides who a message is for. This decides what actually happens
// as a result, and owns every rule that both adapters must agree on: one turn
// at a time per agent, one wake per message per recipient, and a mark so
// restarting Consortium does not replay a week of conversation as fresh work.
//
// Adapters own a process and a translation. They do not queue, deduplicate, or
// decide whether they should have been woken — if they did, two adapters could
// hold different opinions about the rules and the difference would only show up
// under load.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};

use crate::agent::{AgentAdapter, AgentState, ContextLine, WakeRequest};
use crate::bus;
use crate::room;
use crate::router::{self, Decision, Envelope};

/// How much of the room a woken agent is given.
///
/// Enough to make the request make sense, not the whole history. The triggering
/// message alone reads as a non-sequitur; the entire log grows without bound
/// and buries the part that matters.
const CONTEXT_LINES: usize = 12;

pub struct AgentManager {
    /// Lowercased name to the thread that runs its turns. One sender per agent
    /// is what makes execution serial: a second wake queues behind the first
    /// rather than starting a concurrent turn against the same working tree.
    queues: HashMap<String, Sender<WakeRequest>>,
    names: Vec<String>,
    /// Message index and recipient already enqueued. A message that reaches
    /// this twice — two watcher events for one write, a rescan after a reload —
    /// must not wake anyone a second time.
    seen: Arc<Mutex<HashSet<(usize, String)>>>,
    /// How far the chat has been read.
    high_water: Arc<Mutex<usize>>,
    /// Held so status can be *asked* rather than assumed. Each adapter is also
    /// owned by its worker thread; the state lives behind the adapter's own
    /// lock, so both see the same answer.
    adapters: Vec<Arc<dyn AgentAdapter>>,
}

impl AgentManager {
    /// Starts a worker per adapter and returns a manager that can route to them.
    ///
    /// The high-water mark starts at the current end of the chat, so first run
    /// wakes nobody for anything already said. A room that answered a week of
    /// backlog the moment it gained the ability to would be a disaster in its
    /// first second.
    pub fn start(adapters: Vec<Arc<dyn AgentAdapter>>) -> Self {
        let names: Vec<String> = adapters.iter().map(|a| a.name().to_lowercase()).collect();
        let mut queues = HashMap::new();

        // Failures already reported, as "agent: reason". A rate limit does not
        // clear because someone sent another message, so repeating it for every
        // message says nothing new and buries the room.
        let announced: Arc<Mutex<HashSet<String>>> = Arc::new(Mutex::new(HashSet::new()));
        let held = adapters.clone();

        for adapter in adapters {
            let (tx, rx) = channel::<WakeRequest>();
            let name = adapter.name().to_lowercase();
            let announced = Arc::clone(&announced);

            std::thread::spawn(move || {
                // One turn at a time, by construction rather than by a lock
                // somebody might forget to take.
                for request in rx {
                    bus::log(&format!(
                        "wake: {} for message {} from {}",
                        request.agent, request.message_index, request.sender
                    ));

                    match adapter.wake(&request) {
                        Ok(Some(reply)) if !reply.trim().is_empty() => {
                            bus::post(adapter.name(), reply.trim());
                        }
                        Ok(_) => {
                            // Nothing to say is a real answer. Posting "ok" here
                            // would be the acknowledgement that wakes somebody
                            // else, which is the loop we are avoiding.
                            bus::log(&format!("wake: {} had nothing to say", request.agent));
                        }
                        Err(e) => {
                            // Always logged: every attempt is worth a line.
                            bus::log(&format!("wake: {} failed: {e}", request.agent));

                            // Said out loud once. A turn that fails silently
                            // looks exactly like an agent choosing not to
                            // answer and the room waits on it forever — but the
                            // same failure repeated for every message is its own
                            // kind of useless.
                            let first = announced
                                .lock()
                                .unwrap()
                                .insert(format!("{}: {e}", request.agent));
                            if first {
                                bus::post(
                                    router::SYSTEM,
                                    &format!("{} could not answer: {e}", request.agent),
                                );
                            }
                        }
                    }
                }
            });

            queues.insert(name, tx);
        }

        let start_at = bus::read_lines().len();
        bus::log(&format!(
            "manager: watching {} agent(s) in {}, from message {start_at}",
            names.len(),
            bus::workspace().display()
        ));

        Self {
            queues,
            names,
            seen: Arc::new(Mutex::new(HashSet::new())),
            high_water: Arc::new(Mutex::new(start_at)),
            adapters: held,
        }
    }

    /// Considers everything written since the last call and wakes whoever the
    /// router says should be woken.
    ///
    /// Driven by the filesystem watcher, so this runs when the chat actually
    /// changes rather than on a timer.
    pub fn poll(&self) {
        let lines = bus::read_lines();
        let mut high = self.high_water.lock().unwrap();
        if lines.len() <= *high {
            return;
        }

        let senders: Vec<String> = lines
            .iter()
            .map(|l| bus::field(l, "from").unwrap_or_default())
            .collect();

        for index in *high..lines.len() {
            let line = &lines[index];
            let Some(from) = bus::field(line, "from") else {
                continue;
            };
            let to: Vec<String> = bus::field(line, "to")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .map(str::to_string)
                .collect();

            // Hops are counted from everything before this message, so a human
            // speaking anywhere in the chain resets the budget.
            let hops = router::hops_since_human(&senders[..index], &self.names);
            let envelope = Envelope {
                from: &from,
                to: &to,
            };

            match router::route(&envelope, &self.names, hops) {
                Decision::Wake(targets) => {
                    for target in targets {
                        self.enqueue(&target, index, hops, &from, line, &lines);
                    }
                }
                Decision::Nobody => {}
                Decision::HopLimit(count) => {
                    bus::log(&format!(
                        "manager: hop limit reached at message {index} ({count} agent turns since a human spoke)"
                    ));
                    // Announced rather than swallowed. Stopping quietly is
                    // indistinguishable from being broken, and the person who
                    // can restart the conversation needs to know it stopped.
                    bus::post(
                        router::SYSTEM,
                        &format!(
                            "Automatic agent replies paused after {count} exchanges without anyone else speaking. \
                             Say something to resume."
                        ),
                    );
                }
            }
        }

        *high = lines.len();
    }

    fn enqueue(
        &self,
        agent: &str,
        index: usize,
        hops: u32,
        sender: &str,
        line: &str,
        lines: &[String],
    ) {
        let key = (index, agent.to_string());
        if !self.seen.lock().unwrap().insert(key) {
            return; // Already enqueued for this agent.
        }

        let Some(queue) = self.queues.get(agent) else {
            return;
        };

        // An agent that is already broken is not woken. Its failure was
        // announced when it happened; waking it to fail again in exactly the
        // same way adds nothing and costs a turn. It comes back when it is
        // restarted, which is a deliberate act rather than a side effect of
        // someone speaking.
        if let Some(adapter) = self
            .adapters
            .iter()
            .find(|a| a.name().to_lowercase() == agent)
        {
            let state = adapter.state();
            if matches!(state, AgentState::Error(_) | AgentState::Offline) {
                bus::log(&format!("manager: not waking {agent}, it is {state}"));
                return;
            }
        }

        let start = index.saturating_sub(CONTEXT_LINES);
        let context = lines[start..index]
            .iter()
            .filter_map(|l| {
                Some(ContextLine {
                    from: bus::field(l, "from")?,
                    text: bus::field(l, "text")?,
                })
            })
            .collect();

        let request = WakeRequest {
            agent: agent.to_string(),
            // Derived from the folder, so it is the same thread here tomorrow
            // and there is nothing to choose or store.
            session: room::session_for(agent),
            message_index: index,
            sender: sender.to_string(),
            body: bus::field(line, "text").unwrap_or_default(),
            context,
            // Passed through rather than zeroed. An agent that cannot see how
            // deep the exchange has run cannot mention it when it declines.
            hops,
            workspace: bus::workspace().to_string_lossy().into_owned(),
        };

        if queue.send(request).is_err() {
            // The worker thread is gone, so this wake will never happen. Better
            // said than silently dropped.
            bus::log(&format!("manager: {agent}'s worker is not running"));
        }
    }

    /// Forgets where it had got to, for a chat that has been emptied or a room
    /// that has moved.
    ///
    /// Both halves matter. The high-water mark is an index into a file that no
    /// longer has that many lines, so leaving it would silently stop every
    /// future wake — the room would look alive and answer nothing. And `seen` is
    /// keyed by index, so the new message 0 would be mistaken for the old one
    /// and suppressed.
    pub fn reset(&self) {
        *self.high_water.lock().unwrap() = bus::read_lines().len();
        self.seen.lock().unwrap().clear();
        bus::log("manager: starting from the current end of the chat");
    }

    /// Shuts every agent down.
    ///
    /// Called when the window closes. An adapter that owns a process must be
    /// told, because on Windows a child does not die with its parent: without
    /// this, closing Consortium left an agent's process running with nothing
    /// attached to it, and every launch started another.
    ///
    /// Each is tried regardless of what the last one did — one adapter
    /// refusing to stop is no reason to leave the others running.
    pub fn stop_all(&self) {
        for adapter in &self.adapters {
            match adapter.stop() {
                Ok(()) => bus::log(&format!("agent: {} stopped", adapter.name())),
                Err(e) => bus::log(&format!(
                    "agent: {} did not stop cleanly: {e}",
                    adapter.name()
                )),
            }
        }
    }

    /// What each agent is actually doing, asked of the adapters.
    ///
    /// This used to return Idle for everyone, which meant a crashed agent looked
    /// exactly like a healthy one and the room had no way to tell. A status line
    /// that cannot be wrong is not a status line.
    pub fn states(&self) -> Vec<(String, AgentState)> {
        self.adapters
            .iter()
            .map(|a| (a.name().to_string(), a.state()))
            .collect()
    }
}
