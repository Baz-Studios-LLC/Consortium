// Who a message should wake, and whether it should wake anyone.
//
// Kept separate from the adapters and free of any process, because these are
// the rules the whole system stands on and they should be arguable without
// starting anything. Every decision here is made from structure the message
// already carries — the sender, and the recipients `post` recorded — never from
// reading the prose. Codex made that argument and it is right: "is this asking
// for work" is semantic, and any heuristic for it eventually suppresses a real
// request or wakes on politeness. The syntax is the contract instead.

/// A message as the router sees it.
pub struct Envelope<'a> {
    /// Position in the log. Message identity everywhere in Consortium.
    pub index: usize,
    pub from: &'a str,
    /// Lowercased names the message addressed, as `post` recorded them.
    pub to: &'a [String],
}

/// How many agent-to-agent wakes may happen before a human speaks again.
///
/// A backstop, not the mechanism. The convention — never @ someone merely to
/// agree — is what should keep this from ever being reached; this exists for
/// when the convention fails, which is a question of when rather than whether.
pub const MAX_AGENT_HOPS: u32 = 8;

/// Consortium's own voice.
///
/// Announcements — a turn that failed, a hop limit reached — are posted under
/// this name so they are visible in the room rather than buried in a log. They
/// must never wake anyone. An error report is not a request, and a router that
/// cannot tell the difference lets a failing agent talk the room into failing
/// forever: the failure is announced, the announcement looks like someone
/// speaking, everyone is woken, and the failure happens again.
pub const SYSTEM: &str = "system";

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    /// Wake these agents, in this order.
    Wake(Vec<String>),
    /// Nobody. The ordinary outcome for an agent talking to the room.
    Nobody,
    /// The hop limit was reached. Carries the count so the room can be told
    /// what happened rather than simply going quiet — a silent limit looks
    /// exactly like a broken system.
    HopLimit(u32),
}

/// Decides who a message wakes.
///
/// `agents` is every agent Consortium can actually reach, lowercased. `hops` is
/// how many agent-authored messages have run since a human last spoke.
pub fn route(message: &Envelope, agents: &[String], hops: u32) -> Decision {
    let from = message.from.to_lowercase();

    // Checked before anything else. Consortium is not a participant, and
    // nothing it says about the room is addressed to the room.
    if from == SYSTEM {
        return Decision::Nobody;
    }

    let sender_is_agent = agents.iter().any(|a| *a == from);

    // Only agent-to-agent traffic is bounded. A human speaking always gets
    // through, and always resets the count — a person in the conversation is
    // the thing that makes a loop not a loop.
    if sender_is_agent && hops >= MAX_AGENT_HOPS {
        return Decision::HopLimit(hops);
    }

    let mentioned: Vec<String> = message
        .to
        .iter()
        .map(|n| n.to_lowercase())
        // Unknown names are dropped here rather than in the parser, so a typo
        // is still visible in the message and can be answered by a human.
        .filter(|n| agents.contains(n))
        // Addressing yourself is not a wake.
        .filter(|n| *n != from)
        .collect();

    if !mentioned.is_empty() {
        let mut unique = Vec::new();
        for name in mentioned {
            if !unique.contains(&name) {
                unique.push(name);
            }
        }
        return Decision::Wake(unique);
    }

    // No mentions. The two cases differ, and the difference is the whole of the
    // loop protection: a person addressing the room expects an answer, and an
    // agent remarking to the room does not require one. "The changes are
    // committed" is a full stop, and treating it as a request is how two
    // agreeable agents end up talking to each other with nobody listening.
    if sender_is_agent {
        Decision::Nobody
    } else {
        Decision::Wake(agents.to_vec())
    }
}

/// How many agent messages have run since a human last spoke.
///
/// Counted from the log rather than tracked in a field, so it cannot drift out
/// of step with what was actually said, and so it survives a restart without
/// anything being persisted.
pub fn hops_since_human(senders: &[String], agents: &[String]) -> u32 {
    let mut hops = 0;
    for from in senders.iter().rev() {
        let from = from.to_lowercase();
        if agents.iter().any(|a| *a == from) {
            hops += 1;
        } else {
            break;
        }
    }
    hops
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agents() -> Vec<String> {
        vec!["claude".to_string(), "codex".to_string()]
    }

    fn msg<'a>(from: &'a str, to: &'a [String]) -> Envelope<'a> {
        Envelope { index: 0, from, to }
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_human_addressing_the_room_wakes_everyone() {
        // "Can you both review this?" — a person expects an answer.
        let m = msg("Brett", &[]);
        assert_eq!(route(&m, &agents(), 0), Decision::Wake(agents()));
    }

    #[test]
    fn a_human_naming_one_agent_wakes_only_that_one() {
        let to = names(&["codex"]);
        let m = msg("Brett", &to);
        assert_eq!(route(&m, &agents(), 0), Decision::Wake(names(&["codex"])));
    }

    #[test]
    fn an_agent_remarking_to_the_room_wakes_nobody() {
        // The rule the whole design rests on. "The changes are committed" is a
        // full stop, not a request.
        let m = msg("Claude", &[]);
        assert_eq!(route(&m, &agents(), 0), Decision::Nobody);
    }

    #[test]
    fn an_agent_naming_another_wakes_it() {
        let to = names(&["codex"]);
        let m = msg("Claude", &to);
        assert_eq!(route(&m, &agents(), 0), Decision::Wake(names(&["codex"])));
    }

    #[test]
    fn consortium_announcing_a_failure_wakes_nobody() {
        // The loop this exists to prevent, which cost a live room a flood of
        // identical errors: a failed turn is announced, the announcement is
        // read as somebody speaking, everyone is woken, and it fails again.
        // Note it wakes nobody even when it names an agent — an error report
        // that mentions Codex is about Codex, not addressed to it.
        let to = names(&["codex"]);
        assert_eq!(route(&msg("system", &to), &agents(), 0), Decision::Nobody);
        assert_eq!(route(&msg("system", &[]), &agents(), 0), Decision::Nobody);
    }

    #[test]
    fn mentioning_yourself_is_not_a_wake() {
        // Otherwise an agent signing off with its own name would wake itself,
        // forever, immediately.
        let to = names(&["claude"]);
        let m = msg("Claude", &to);
        assert_eq!(route(&m, &agents(), 0), Decision::Nobody);
    }

    #[test]
    fn unknown_names_wake_nobody_but_do_not_swallow_the_message() {
        // @Gemini with no Gemini present: nothing is woken, and the decision is
        // the same as any unaddressed agent message rather than an error.
        let to = names(&["gemini"]);
        let m = msg("Claude", &to);
        assert_eq!(route(&m, &agents(), 0), Decision::Nobody);
    }

    #[test]
    fn the_hop_limit_stops_agents_but_never_a_person() {
        let to = names(&["codex"]);
        let from_agent = msg("Claude", &to);
        assert_eq!(
            route(&from_agent, &agents(), MAX_AGENT_HOPS),
            Decision::HopLimit(MAX_AGENT_HOPS)
        );

        // A human at the same hop count still gets through. Otherwise the limit
        // would lock the room against the one participant who can fix it.
        let from_human = msg("Brett", &to);
        assert_eq!(
            route(&from_human, &agents(), MAX_AGENT_HOPS),
            Decision::Wake(names(&["codex"]))
        );
    }

    #[test]
    fn hops_count_back_to_the_last_human() {
        let a = agents();
        assert_eq!(hops_since_human(&names(&["Brett"]), &a), 0);
        assert_eq!(hops_since_human(&names(&["Brett", "Claude"]), &a), 1);
        assert_eq!(
            hops_since_human(&names(&["Brett", "Claude", "Codex", "Claude"]), &a),
            3
        );
        // A person speaking resets it, which is what makes a conversation with
        // a human in it unbounded.
        assert_eq!(
            hops_since_human(&names(&["Claude", "Codex", "Brett"]), &a),
            0
        );
    }

    #[test]
    fn names_are_matched_without_regard_to_case() {
        let to = names(&["CODEX"]);
        let m = msg("BRETT", &to);
        assert_eq!(route(&m, &agents(), 0), Decision::Wake(names(&["codex"])));
    }
}
