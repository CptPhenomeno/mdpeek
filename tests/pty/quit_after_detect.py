"""Runs readmd in a pty that never answers terminal queries, with auto detection,
and checks that the first 'q' quits it (KB-84). Usage: quit_after_detect.py BINARY FILE.
Exit 0 when it quits, 1 when the key was swallowed."""
import fcntl, os, pty, select, struct, sys, termios, time

binary, path = sys.argv[1], sys.argv[2]
pid, fd = pty.fork()
if pid == 0:
    os.environ["TERM"] = "xterm-256color"
    for var in ("WEZTERM_EXECUTABLE", "KONSOLE_VERSION", "TERM_PROGRAM", "TMUX"):
        os.environ.pop(var, None)
    os.execv(binary, [binary, path])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))


def drain(seconds):
    end = time.time() + seconds
    while time.time() < end:
        if select.select([fd], [], [], 0.02)[0]:
            try:
                if not os.read(fd, 65536):
                    return
            except OSError:
                return


drain(1.5)  # first frame, detection timeout and grace are over
os.write(fd, b"q")
end = time.time() + 3
while time.time() < end:
    done, _ = os.waitpid(pid, os.WNOHANG)
    if done:
        sys.exit(0)
    drain(0.05)
os.kill(pid, 9)
os.waitpid(pid, 0)
print("quit after q: stuck", file=sys.stderr)
sys.exit(1)
