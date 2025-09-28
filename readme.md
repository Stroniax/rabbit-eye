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

## Contributing

Use `<repo-root>/.cargo/config.toml` to 
[configure the environment](https://doc.rust-lang.org/cargo/reference/config.html#configuration-format)
which propagates to running a program in development (`cargo run`). This file specifically
is in the `.gitignore`. Other `config.toml` files may be tracked and contain shared application
configurations.