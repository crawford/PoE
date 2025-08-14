# API #

This registers an event handler through the SVC interface and then triggers
it. A message is printed (using another SVC call) when the handler runs,
demonstrating support for reentrancy.

Here's what the output should look like, with some additional debug output:

    DEBUG src/call.rs:487 - RegisterHandler(0xabcd, 0x200044cd) @ 0x20002928
    DEBUG src/call.rs:209 - TriggerEvent(0xabcd)
    DEBUG src/call.rs:222 -  looking for 0xabcd, after 0x0
    DEBUG src/call.rs:231 -   found at 0x20002928: 0x200044cd
    DEBUG src/call.rs:222 -  looking for 0xabcd, after 0x20002928
    DEBUG src/call.rs:366 - Calling Handler: 0x200044cd
    INFO  src/call.rs:495 - Hello, world!
    DEBUG src/call.rs:371 - Called Handler: 0x200044cd
