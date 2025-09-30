# Rabbit-Eye

Rabbit-Eye is a monitoring tool designed to observe changes in a system and publish change notifications
to RabbitMQ. Tooling will be developed for many systems, including the following:

- FileSystem
- FTP/SFTP
- Database (table/view/query)

## Runtime Interaction

The observer is a standalone executable daemon application. There is support for some POSIX signals or
Windows service events.

| POSIX Signal | Windows Service Control Event | Action |
| SIGTERM | | Terminate the application |
| SIGINT | | Terminate the application |
| | | Suspend the polling, keep the app running |
| SIGCONT | | Resume polling when suspended |

## Development Perspective

There are two forms that rabbit-eye events may be produced.

1. Schedule-based
   A timer prompts an observer to scan a data set. The data is hashed and compared with the previous
   set of data. Changes are published.
2. Event-based
   An OS or application event is raised. For example a filesystemwatcher may raise events,
   or a network endpoint may receive messages, that then get turned into RabbitMq messages.

### State

Applications can define their own state management system to use. The following guidance explains
how default state is managed.

#### Persistence

```rust
// mod rabbit_eye::state;

trait StatePersistence {
    /// Load the persisted state. This should be a highly tolerant method; if state is corrupted
    /// it is better to reset than to return an error because the application will not be able
    /// to resolve an error and will not be able to run.
    async fn load() -> Result<Self, Box<dyn Error>>;

    /// Persist the state for loading later by `load`.
    async fn save(self) -> Result<(), Box<dyn Error>>;

    /// Whether the application should reload the state between iterations or may maintain
    /// a cached copy in memory.
    fn retain() -> bool;
}
```

The application host can use the state in the following way

## Contributing

Use `<repo-root>/.cargo/config.toml` to
[configure the environment](https://doc.rust-lang.org/cargo/reference/config.html#configuration-format)
which propagates to running a program in development (`cargo run`). This file specifically
is in the `.gitignore`. Other `config.toml` files may be tracked and contain shared application
configurations.

Launch the `message-to-console` app (intended for debugging, not to be published) to
observe messages produced by a `rabbit-eye` observer.

```PowerShell
PS \> cargo run --bin message-to-console
```
