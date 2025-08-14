# README #

## Development Tips ##

If openocd starts reporting bunch of errors about SWO, it seems to be because
the J-Link debugger is overwhelmed with output from the ITM. Commenting out the
TPIU configuration in `openocd.cfg` allows the debugger to attach again. You
might have to power-cycle the board immediately before programming - idea being
that the SWO buffer hasn't had a chance to fill up yet.

After building the binary, run `arm-none-eabi-gdb`, which will start OpenOCD and
connect to it through a pipe. If you see the following error, try power-cycling:

    Remote communication error. Target disconnected:
                                error while reading: Connection reset by peer.

## Architecture ##

The system is split up into two components: the supervisor and the
application. The supervisor is responsible for the initialization of the MCU,
interrupt and exception handling, network configuration and communication, and
the full lifecycle of the application. The application provides custom behavior,
making use of syscalls (Supervisor Calls) to direct the supervisor.

### Events and Handlers ###

Core to this idea are events and handlers. Handlers are functions provided by
the application and are registered to events. Events are triggered by calls to
TriggerEvent, which runs all of the associated handlers. This callback system
allows the supervisor to call asynchronously into the application.

### Application Lifecycle ###

Each application is executed, in turn, once the supervisor is satisfied with the
state of the system. In the case of a long-running application, the code should
not block. Instead, it should use syscalls to configure resources and register
event handlers, exiting afterward. Once the initial application code returns,
the application can be thought of as simply a library of handlers which are
called in response to events.

### Syscalls ##

Syscalls are made by executing `SVC` after loading the procedure ID and any
arguments according to the Procedure Call Standard for Arm Architecture (AAPCS).

#### TriggerEvent ####

TriggerEvent is a special syscall. This is the one that is used to run the
handlers for an event. Because it causes application code to run, its flow is
different from the other syscalls.

When TriggerEvent is called, the SVCall exception handler does the following:

1. Move the stacked exception frame down the stack to make room for the list of
   handlers. Right now, this list is a hard-coded length.
2. Find every handler associated with the event, and put them into the list of
   handlers on the stack, making sure to null-terminate it.
3. Return from SVCall, _but resume execution in a helper thunk_ which pops the
   list of handlers off the stack and executes them in turn. When this function
   returns, it jumps back to the caller of the syscall.

#### Alternatives ###

##### Vector #####

Syscalls can be made by the application to certain functions within the
supervisor. These functions are grouped into a vector which is versioned
according to its layout, the functions' signatures, and their behavior. When the
application calls into the supervisor, it specifies which vector it would like
to target in the argument to the `SVC` instruction. The specific syscall number
is then specified in `r0`, with the arguments following.

```asm
    .global syscall
syscall:
    SVC #1

    .global log
log:
    b syscall
```

`r0` needs to be reserved at the call-site (dummy variable, handle to something)
and at the syscall implementation so that the application library can use it to
specify the syscall number.

##### Other #####

The other method of specifying the syscall number is in the argument to
`SVC`. In this scheme, the syscall signature is simplified since it no longer
need to specify the syscall number. As new syscalls are added, they can fill in
the remaining space. Old syscalls could be removed, freeing up their numbers for
reuse. This method requires two memory dereferences for every syscall: the first
one retrieving the link register from the stack and the second, the argument to
`SVC`. This method could also benefit from having an external version scheme
that dictates which syscall numbers are guaranteed to be
implemented. Alternatively, the application library could check the validity of
each syscall before using it, possibly making use of a rewriting trampoline
function to optimize.
