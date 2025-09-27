# Rabbit-Eye

Rabbit-Eye is a monitoring tool designed to observe changes in a system and publish change notifications
to RabbitMQ. Tooling will be developed for many systems, including the following:

- FileSystem
- FTP/SFTP
- Database (table/view/query)

The observer is a standalone executable daemon application. There is support for some POSIX signals or
Windows service events.

| POSIX Signal | Windows Service Control Event | Action |
| SIGTERM | | Terminate the application |
| SIGINT | | Terminate the application |
| | | Suspend the polling, keep the app running |
| SIGCONT | | Resume polling when suspended |