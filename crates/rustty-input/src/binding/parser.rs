//! PORTS: ghostty src/input/Binding.zig — `Parser` (the binding-string iterator), `parseFlags`,
//! `SequenceIterator` and `parseSingle`.

use super::*;

// ===========================================================================
// Parser
// ===========================================================================

/// PORT NOTE: `Parser.Elem` — the union yielded by the parser iterator.
#[derive(Clone, Debug, PartialEq)]
pub enum Elem {
    /// A leader trigger in a sequence.
    Leader(Trigger),

    /// The final trigger and action in a sequence.
    Binding(Binding),

    /// A chained action `chain=<action>` that should be appended
    /// to the previous binding. Note that any action is parsed, including
    /// invalid actions for chains such as `unbind`. We expect downstream
    /// consumers to validate that the action is valid for chaining.
    Chain(Action),
}

/// Full binding parser. The binding parser is implemented as an iterator
/// which yields elements to support multi-key sequences without allocation.
pub struct Parser {
    pub trigger_it: SequenceIterator,
    pub action: Action,
    pub flags: Flags,
    pub chain: bool,
}

impl Parser {
    pub fn init(raw_input: &[u8]) -> Result<Parser, Error> {
        // const flags, const start_idx = try parseFlags(raw_input);
        let (flags, start_idx) = parse_flags(raw_input)?;
        let input = &raw_input[start_idx..];

        // Find the equal sign. This is more complicated than it seems on
        // the surface because we need to ignore equal signs that are
        // part of the trigger.
        let eql_idx: usize = 'eql: {
            // TODO: We should change this parser into a real state machine
            // based parser that parses the trigger fully, then yields the
            // action after. The loop below is a total mess.
            let mut offset: usize = 0;
            while let Some(offset_idx) = input[offset..].iter().position(|&c| c == b'=') {
                // Find: '=+ctrl' or '==action'
                let idx = offset + offset_idx;
                if idx < input.len() - 1 && (input[idx + 1] == b'+' || input[idx + 1] == b'=') {
                    offset += offset_idx + 1;
                    continue;
                }

                // Looks like the real equal sign.
                break 'eql idx;
            }

            return Err(Error::InvalidFormat);
        };

        // Detect chains. Chains must not have flags.
        let chain = &input[0..eql_idx] == b"chain";
        if chain && start_idx > 0 {
            return Err(Error::InvalidFormat);
        }

        // Sequence iterator goes up to the equal, action is after. We can
        // parse the action now.
        Ok(Parser {
            trigger_it: SequenceIterator {
                // This is kind of hacky but we put a dummy trigger
                // for chained inputs. The `next` will never yield this
                // because we have chain set. When we find a nicer way to
                // do this we can remove it, the e2e is tested.
                input: if chain {
                    b"a".to_vec()
                } else {
                    input[0..eql_idx].to_vec()
                },
                i: 0,
            },
            action: Action::parse(&input[eql_idx + 1..])?,
            flags,
            chain,
        })
    }

    // PORT NOTE: named `next` to mirror the reference `Parser.next`; it is not an
    // `Iterator::next` (different signature/semantics — fallible, yields `Elem`).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Elem>, Error> {
        // Get our trigger. If we're out of triggers then we're done.
        let trigger = match self.trigger_it.next()? {
            Some(t) => t,
            None => return Ok(None),
        };

        // If this is our last trigger then it is our final binding.
        if !self.trigger_it.done() {
            // Global/all bindings can't be sequences
            if self.flags.global || self.flags.all {
                return Err(Error::InvalidFormat);
            }
            return Ok(Some(Elem::Leader(trigger)));
        }

        // If we're a chain then return it as-is.
        if self.chain {
            return Ok(Some(Elem::Chain(self.action.clone())));
        }

        // Out of triggers, yield the final action.
        Ok(Some(Elem::Binding(Binding {
            trigger,
            action: self.action.clone(),
            flags: self.flags,
        })))
    }

    pub fn reset(&mut self) {
        self.trigger_it.i = 0;
    }
}

// fn parseFlags(raw_input) Error!struct { Flags, usize }
fn parse_flags(raw_input: &[u8]) -> Result<(Flags, usize), Error> {
    let mut flags: Flags = Flags::default();

    let mut start_idx: usize = 0;
    let mut input: &[u8] = raw_input;
    // PORT NOTE: mirrors the reference `while (true) { ... orelse break; }`.
    #[allow(clippy::while_let_loop)]
    loop {
        // Find the next prefix
        let idx = match input.iter().position(|&c| c == b':') {
            Some(i) => i,
            None => break,
        };
        let prefix = &input[0..idx];

        // If the prefix is one of our flags then set it.
        if prefix == b"all" {
            if flags.all {
                return Err(Error::InvalidFormat);
            }
            flags.all = true;
        } else if prefix == b"global" {
            if flags.global {
                return Err(Error::InvalidFormat);
            }
            flags.global = true;
        } else if prefix == b"unconsumed" {
            if !flags.consumed {
                return Err(Error::InvalidFormat);
            }
            flags.consumed = false;
        } else if prefix == b"performable" {
            if flags.performable {
                return Err(Error::InvalidFormat);
            }
            flags.performable = true;
        } else {
            // If we don't recognize the prefix then we're done. We
            // let any unknown prefix fallthrough to trigger-specific
            // parsing in case there are trigger-specific prefixes
            // (none currently but historically there was `physical:`
            // at one point). Breaking here lets us always implement new
            // prefixes.
            break;
        }

        // Move past the prefix
        start_idx += idx + 1;
        input = &input[idx + 1..];
    }

    Ok((flags, start_idx))
}

/// An iterator that yields each trigger in a sequence of triggers. For
/// example, the sequence "ctrl+a>ctrl+b" would yield "ctrl+a" and then
/// "ctrl+b". The iterator approach allows us to parse a sequence of
/// triggers without allocations.
//
// PORT NOTE: the reference holds `input: []const u8` (a borrow). Here the
// `Parser` owns the trigger substring as a `Vec<u8>` so the iterator is self-
// contained (the reference's "no allocations" is a performance property, not an
// observable one — preserved as a `Goal-2` candidate, not a parity invariant).
pub struct SequenceIterator {
    /// The input of triggers. This is expected to be ONLY triggers. Things
    /// like the "unconsumed:" prefix or action must be stripped before
    /// passing to this iterator.
    pub input: Vec<u8>,
    pub i: usize,
}

impl SequenceIterator {
    /// Returns the next trigger in the sequence if there is no parsing error.
    // PORT NOTE: named `next` to mirror `SequenceIterator.next`; fallible and not
    // an `Iterator::next`.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Result<Option<Trigger>, Error> {
        if self.done() {
            return Ok(None);
        }
        let rem = &self.input[self.i..];
        let idx = rem.iter().position(|&c| c == b'>').unwrap_or(rem.len());
        // defer self.i += idx + 1;
        let result = Trigger::parse(&rem[0..idx]);
        self.i += idx + 1;
        Ok(Some(result?))
    }

    /// Returns true if there are no more triggers to parse.
    pub fn done(&self) -> bool {
        self.i > self.input.len()
    }
}

/// Parse a single, non-sequenced binding. To support sequences you must
/// use parse. This is a convenience function for single bindings aimed
/// primarily at tests.
///
/// This doesn't support `chain` either, since chaining requires some
/// stateful concept of a prior binding.
//
// PORT NOTE: the reference error set is `Error || error{ UnexpectedChain,
// UnexpectedSequence }`. Modeled as `ParseSingleError`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseSingleError {
    InvalidFormat,
    InvalidAction,
    UnexpectedChain,
    UnexpectedSequence,
}

impl From<Error> for ParseSingleError {
    fn from(e: Error) -> ParseSingleError {
        match e {
            Error::InvalidFormat => ParseSingleError::InvalidFormat,
            Error::InvalidAction => ParseSingleError::InvalidAction,
        }
    }
}

pub fn parse_single(raw_input: &[u8]) -> Result<Binding, ParseSingleError> {
    let mut p = Parser::init(raw_input)?;
    let elem = match p.next()? {
        Some(e) => e,
        None => return Err(ParseSingleError::InvalidFormat),
    };
    match elem {
        Elem::Leader(_) => Err(ParseSingleError::UnexpectedSequence),
        Elem::Binding(b) => Ok(b),
        Elem::Chain(_) => Err(ParseSingleError::UnexpectedChain),
    }
}
