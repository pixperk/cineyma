# tcp chat server

a simple multi-user chat server demonstrating cinema's stream handling and actor communication.

## features

- **stream integration**: tcp streams as actor message sources
- **actor coordination**: chatserver broadcasts to multiple clientsession actors
- **lifecycle hooks**: automatic connect/disconnect handling
- **async i/o**: non-blocking tcp read/write operations

## architecture

- `ChatServer`: central coordinator managing all connected clients
  - maintains client registry
  - broadcasts messages to all participants
  - handles connect/disconnect events

- `ClientSession`: one actor per tcp connection
  - reads incoming lines via stream handler
  - sends outgoing messages via async writer task
  - auto-registers with chatserver on start
  - auto-disconnects on stop

## running the example

### start the server

```bash
cargo run -p chat
```

### connect clients

```bash
nc localhost 8080
```

or use telnet:

```bash
telnet localhost 8080
```

## example session

**terminal 1 (server):**
```
Chat server started on 127.0.0.1:8080
Connect with: nc localhost 8080
[server] ChatServer started
New connection from 127.0.0.1:54321 as user1
[server] user1 joined
```

**terminal 2 (client 1):**
```
$ nc localhost 8080
* user1 joined the chat
hello everyone!
[user2]: hi there!
```

**terminal 3 (client 2):**
```
$ nc localhost 8080
* user2 joined the chat
* user1 joined the chat
[user1]: hello everyone!
hi there!
```

## what this demonstrates

1. **stream handling**: tcp socket integrated as actor message stream
2. **message broadcasting**: chatserver fans out messages to all clients
3. **lifecycle management**: actors auto-register and cleanup on start/stop
4. **actor coordination**: multiple actors communicate via messages
5. **async i/o separation**: read stream in actor, write task outside actor system

## implementation details

- **port**: 8080
- **username**: auto-assigned as `userN` where N increments per connection
- **message format**: `[username]: message` for broadcasts, `* username action` for system events
- **disconnect**: happens automatically when client closes connection or on error
