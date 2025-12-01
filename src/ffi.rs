//! FFI functions for actor builtins
//!
//! These are the C-callable functions that get linked into compiled Seq programs.
//! They bridge between Seq code and the actor runtime.
//!
//! # Linking
//!
//! When a Seq program uses actor builtins like `actor-spawn`, the compiler
//! generates calls to these `seq_actors_*` functions. At link time, this
//! library provides the implementations.
//!
//! # Stack Convention
//!
//! All functions follow seq-runtime's stack convention:
//! - Take a `Stack` (pointer to stack top)
//! - Return a `Stack` (new stack top after operation)
//! - Stack grows downward (push = allocate node, set next = old top)

#![allow(dead_code)] // FFI functions used at link time, not called from Rust
#![allow(private_interfaces)] // Stack is opaque pointer for C FFI

use crate::actor::ActorId;
use crate::runtime::{get_current_actor, Mailbox, REGISTRY};

// FFI types matching seq-runtime
type Stack = *mut StackNode;

#[repr(C)]
struct StackNode {
    value: Value,
    next: Stack,
}

/// Opaque Value type - we only need to pass it through to seq-runtime
/// The actual Value is defined in seq-runtime, we just handle pointers
#[repr(C)]
union Value {
    int_val: i64,
    _padding: [u8; 32], // Match seq-runtime's Value size
}

// External seq-runtime functions we call
extern "C" {
    fn patch_seq_make_channel(stack: Stack) -> Stack;
    fn patch_seq_chan_send(stack: Stack) -> Stack;
    fn patch_seq_chan_receive(stack: Stack) -> Stack;
    fn patch_seq_close_channel(stack: Stack) -> Stack;
    fn patch_seq_strand_spawn(entry: extern "C" fn(Stack) -> Stack, initial_stack: Stack) -> i64;
    fn patch_seq_push_int(stack: Stack, value: i64) -> Stack;
    fn patch_seq_push_string(stack: Stack, s: *const i8) -> Stack;
}

/// Actor spawn - create a new actor
///
/// Stack: ( behavior_name -- actor_id )
///
/// Creates a new actor with the given behavior and returns its ID.
/// The actor runs as a may coroutine with its own mailbox.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_actors_spawn(stack: Stack) -> Stack {
    // For MVP, we create the actor infrastructure but behavior execution
    // requires more integration with seq-runtime's quotation system.
    //
    // Current implementation:
    // 1. Generate ActorId
    // 2. Create mailbox channel
    // 3. Register in registry
    // 4. Return actor ID as string
    //
    // TODO: Actually spawn coroutine with behavior loop

    // Pop behavior name from stack (we'll use it later)
    let (stack, _behavior) = pop_value(stack);

    // Generate actor ID
    let actor_id = ActorId::new();
    let id_string = actor_id.as_str();

    // Create mailbox channel
    let temp_stack = patch_seq_make_channel(std::ptr::null_mut());
    let (_, channel_id) = pop_int(temp_stack);

    // Register actor
    let mailbox = Mailbox::new(channel_id);
    REGISTRY.register(actor_id, mailbox, "behavior".to_string());

    // Push actor ID string onto stack
    let c_string = std::ffi::CString::new(id_string).expect("actor ID should be valid");
    patch_seq_push_string(stack, c_string.as_ptr())
}

/// Actor send - send a message to an actor
///
/// Stack: ( actor_id message -- )
///
/// Sends a message to the specified actor's mailbox.
/// This is non-blocking (message is queued).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_actors_send(stack: Stack) -> Stack {
    // Pop actor ID and message
    // For now, we pass through to channel send
    // Stack has: ... message actor_id
    // Channel send expects: ... value channel_id

    // The message is already on the stack in the right position
    // We just need to look up the actor's mailbox channel ID

    // Pop actor ID (string)
    let (stack, _actor_id_val) = pop_value(stack);

    // TODO: Look up actor in registry, get mailbox channel ID
    // For now, this is a stub that just drops the message

    // In full implementation:
    // 1. Parse actor ID from string
    // 2. Look up in registry
    // 3. Get mailbox channel ID
    // 4. Push channel ID, call patch_seq_chan_send

    stack
}

/// Actor self - get current actor's ID
///
/// Stack: ( -- actor_id )
///
/// Returns the ID of the currently executing actor.
/// Panics if called outside an actor context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_actors_self(stack: Stack) -> Stack {
    match get_current_actor() {
        Some(id) => {
            let id_string = id.as_str();
            let c_string = std::ffi::CString::new(id_string).expect("actor ID should be valid");
            patch_seq_push_string(stack, c_string.as_ptr())
        }
        None => {
            panic!("actor-self called outside actor context");
        }
    }
}

/// Actor stop - stop an actor
///
/// Stack: ( actor_id -- )
///
/// Signals an actor to stop. The actor will finish processing
/// its current message before stopping.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_actors_stop(stack: Stack) -> Stack {
    // Pop actor ID
    let (stack, _actor_id_val) = pop_value(stack);

    // TODO: Look up actor, send stop signal
    // For now, this is a stub

    stack
}

/// Actor state - get current actor's state
///
/// Stack: ( -- state )
///
/// Returns the current actor's state Map.
/// Panics if called outside an actor context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_actors_state(stack: Stack) -> Stack {
    // TODO: Return actor's current state
    // For now, return empty map (need integration with seq-runtime map creation)
    stack
}

/// Journal append - persist an event
///
/// Stack: ( event -- )
///
/// Persists an event to the current actor's journal.
/// Must be called from within an actor context.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn seq_actors_journal_append(stack: Stack) -> Stack {
    // Pop event from stack
    let (stack, _event_val) = pop_value(stack);

    // TODO: Convert to Event, persist via journal
    // Requires actor context and journal reference

    stack
}

// Helper functions for stack manipulation

unsafe fn pop_value(stack: Stack) -> (Stack, Value) {
    if stack.is_null() {
        panic!("Stack underflow");
    }
    let node = &*stack;
    let value = std::ptr::read(&node.value);
    let next = node.next;
    // Note: In real impl, return node to pool
    (next, value)
}

unsafe fn pop_int(stack: Stack) -> (Stack, i64) {
    let (stack, value) = pop_value(stack);
    (stack, value.int_val)
}

#[cfg(test)]
mod tests {
    // FFI tests require linking with seq-runtime
    // These are integration tests that run with the full stack
}
