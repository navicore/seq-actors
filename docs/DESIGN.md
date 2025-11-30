# seq-actors Design Document

An actor system for Seq with event-sourced persistence.

## Overview

seq-actors extends the Seq compiler to support actor-based concurrency with:
- Actor identity and lifecycle management
- Message passing between actors
- Event-sourced persistence for actor state
- Supervision trees for fault tolerance

## Core Concepts

### Actor Model in Seq

An actor in Seq is:
- **Identity**: A unique ID (UUID or path-based)
- **State**: A Map representing current actor state
- **Behavior**: A quotation that handles messages
- **Journal**: Persisted events for recovery

```seq
# Actor behavior: receives state and message, returns new state
: my-actor ( State Msg -- State' )
  dup variant-tag
  "Deposit" string-equal if
    handle-deposit
  else
    "Withdraw" string-equal if
      handle-withdraw
    then
  then
;

# Message handler
: handle-deposit ( State Msg -- State' )
  1 variant-field-at           # get amount from (Deposit amount)
  over "balance" map-get add   # add to balance
  "balance" swap map-set       # update state
;
```

### Stack as Working Memory, Map as State

The stack is ephemeral working memory during message processing.
The actor's persistent state is a Map, making it:
- Easy to serialize (keys are strings/ints)
- Easy to inspect and debug
- Natural for key-value state patterns

---

## Open Design Questions

### 1. Event Journal Persistence

**Options:**

#### A. File-per-actor (append-only log) ← Current implementation
```
actors/
  {actor-id}/
    journal.bin        # length-prefixed bincode events
    snapshot.bin       # periodic snapshot (bincode)
```

**Pros:**
- Simple, no dependencies
- Debug via `Journal::dump_debug()` or `TypedValue::to_debug_string()`
- Natural for backup/restore
- Portable, fast, compact (bincode)

**Cons:**
- No transactions across actors
- Manual compaction needed
- File locking for concurrent access

#### B. SQLite (single database)
```sql
CREATE TABLE events (
  id INTEGER PRIMARY KEY,
  actor_id TEXT NOT NULL,
  sequence_num INTEGER NOT NULL,
  event_type TEXT NOT NULL,
  payload JSON NOT NULL,
  timestamp INTEGER NOT NULL,
  UNIQUE(actor_id, sequence_num)
);

CREATE TABLE snapshots (
  actor_id TEXT PRIMARY KEY,
  sequence_num INTEGER NOT NULL,
  state JSON NOT NULL
);
```

**Pros:**
- ACID transactions
- Efficient queries (by actor, by time range)
- Built-in compaction via snapshots
- Single file, easy to move

**Cons:**
- Dependency on rusqlite
- Slightly more complex setup

#### C. Hybrid: File journal + SQLite index
Use append-only files for events, SQLite for metadata/indexes.

**Recommendation:** Start with **file-per-actor** for simplicity. Add SQLite later if we need cross-actor queries or transactions.

---

### 2. Serialization Format ✓ DECIDED

**Decision: bincode for journal storage**

The journal is an internal implementation detail for actor resurrection.
External systems access history through the actor's API, not raw journal data.

#### Why bincode?
- **Speed**: Fast writes during operation, fast reads on recovery
- **Compactness**: Less disk I/O, smaller storage
- **Type safety**: Native Rust types, no parsing ambiguity

#### Why not JSON?
- Verbose (quotes, braces, type markers)
- Slow to parse
- Human-readable debugging can use `TypedValue::to_debug_string()` instead

#### Why not Seq stdlib JSON?
The seq stdlib `json` module is for Seq programs at runtime.
Here we're serializing from Rust (the actor runtime), so serde/bincode is appropriate.

#### Value type mapping:
- `Int` → i64
- `Float` → f64
- `Bool` → bool
- `String` → length-prefixed bytes
- `Map` → BTreeMap with typed keys (Int, Bool, String)
- `Variant` → tag string + field array
- `Quotation` → **Error** (behavior is code, state is data)

---

### 3. Supervision and Lifecycle

**Actor lifecycle states:**
```
Created → Running → (Suspended | Failed | Stopped)
                         ↓
                    Restarting
```

**Supervision strategies:**
- `one-for-one`: Restart only the failed actor
- `one-for-all`: Restart all children if one fails
- `rest-for-one`: Restart failed actor and all actors started after it

**Questions:**
- How do we specify supervision trees in Seq?
- Should supervision be declarative (config) or programmatic (Seq code)?
- How do we handle distributed supervision across nodes?

**Initial approach:** Start with simple `one-for-one` restart. Add supervision trees later.

---

### 4. Actor Addressing

**Options:**

#### A. UUID-based
```seq
make-actor [ my-behavior ] actor-spawn   # Returns UUID
"550e8400-e29b-41d4-a716-446655440000" actor-send
```

#### B. Path-based (like Akka)
```seq
"/user/accounts/account-123" actor-spawn
"/user/accounts/account-123" "deposit" 100 make-variant actor-send
```

#### C. Named + hierarchical
```seq
"account-service" [ my-behavior ] actor-register
"account-service" my-message actor-send
```

**Recommendation:** Start with UUIDs, add path-based addressing for supervision trees.

---

### 5. Distributed Features (Future)

**Considerations for future distribution:**

- **Location transparency**: Same addressing whether local or remote
- **Cluster membership**: How actors discover each other
- **Partition tolerance**: What happens when nodes can't communicate
- **Event replication**: How journals sync across nodes

**Initial approach:** Design for single-node, but keep APIs location-agnostic.

---

## Proposed Builtins

### Actor Management
```
actor-spawn     ( Behavior -- ActorId )      # Create new actor
actor-send      ( ActorId Msg -- )           # Send message (fire-and-forget)
actor-ask       ( ActorId Msg -- Response )  # Send and wait for reply
actor-self      ( -- ActorId )               # Current actor's ID
actor-stop      ( ActorId -- )               # Stop an actor
```

### State & Events
```
actor-state     ( -- State )                 # Get current state (Map)
journal-append  ( Event -- )                 # Persist event to journal
```

### Supervision (Future)
```
actor-watch     ( ActorId -- )               # Monitor another actor
actor-unwatch   ( ActorId -- )               # Stop monitoring
supervisor-start ( Strategy Children -- SupervisorId )
```

---

## Implementation Phases

### Phase 1: Foundation
- [ ] Value serialization (serde for Seq Values)
- [ ] File-based event journal
- [ ] Basic actor runtime (spawn, send, state)
- [ ] Integration with seq-compiler

### Phase 2: Persistence
- [ ] Event replay on actor restart
- [ ] Periodic snapshots
- [ ] Journal compaction

### Phase 3: Supervision
- [ ] Actor lifecycle management
- [ ] Basic supervision (one-for-one restart)
- [ ] Error handling and recovery

### Phase 4: Distribution (Future)
- [ ] Remote actor messaging
- [ ] Cluster membership
- [ ] Distributed supervision

---

## Questions for Discussion

1. **Persistence**: File-per-actor vs SQLite vs hybrid?
2. ~~**Serialization**: JSON with string-converted keys acceptable?~~ ✓ **DECIDED: bincode**
3. ~~**Quotations in state**: Error, or store as reference?~~ ✓ **DECIDED: Error** (state is data, behavior is code)
4. **Supervision**: Declarative config or Seq code?
5. **Addressing**: UUIDs only, or path-based from start?

---

## References

- [Erlang/OTP Supervision](https://www.erlang.org/doc/design_principles/sup_princ.html)
- [Akka Actor Model](https://doc.akka.io/docs/akka/current/typed/guide/actors-intro.html)
- [Event Sourcing Pattern](https://martinfowler.com/eaaDev/EventSourcing.html)
- [Cloudstate (now Kalix)](https://docs.kalix.io/) - Event sourcing for actors
