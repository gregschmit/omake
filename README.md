# Make (oxidized)

This is a Rust implementation of `make`, striving to be simple, portable, and fast.

If you want to avoid clashing with your system's `make`, feel free to name this program `omake`
(following the convention of `bmake` and `gmake`).

I decided to try to re-write `make` in Rust both as a way to learn Rust and also because I found the
existing `make` implementations' source code very convoluted.

https://xkcd.com/2314/

## Project Goals

This project is in it's infancy, so I may find out later that some or all of the project goals are
impossible to achieve. Regardless, in order of importance, here are the project goals:

1. Portable makefiles should behave as users expect.
2. Support as many commonly-used BSD and GNU `make` extensions as possible.
3. Be capable of building the Linux kernel.
4. Be really fast.
5. If we decide to implement new extensions, they should be opt-in to retain backwards
   compatibility. We should avoid this unless there are serious performance improvements.
6. Possibly the hardest: don't turn into a backwards-incompatible competing standard
   (https://xkcd.com/927/). As uninspired as it may seem, I just want an implementation of `make`
   that works on Linux and FreeBSD; that's it.

Note that due to implementation details (and especially during the initial development phase this
project is in), it's possible certain features are inadvertently added. Users should probably not
rely on those and they may even qualify as bugs. I hope to get everything ironed out before the 1.0
release to avoid (as stated in Goal #6) building an incompatible competing standard. There are
already other build systems, I don't actually want to make another one.

Working list of things that I plan on leaving out of this implementation intentionally:
1. (GNU) Remaking makefiles from RCS/SCCS. I see no need to support this.
2. (GNU) Implicit rules. I'm on the fence, so I could be convinced to remove this from the list. The
   Linux kernel's makefile explicitly disables implicit rules (ref:
   https://github.com/torvalds/linux/blob/15b3f48a4339e3c16acf18624e2b7f60bc5e9a2c/Makefile#L202-L208).
   However, other projects might use these a lot, so if I find that's the case I might decide to
   implement implicit rules, especially if it's not too cumbersome.

## Testing Methodology (TODO)

I want to unit test as much of the code as possible.

After that, I want to build a series of "system" tests where we use the compiled binary on a series
of makefiles and progammatically check the output (both stdout and filesystem output). I probably
need to figure out how to check if other files are created by mistake and how to purge them.

I should probably also copy over the GNU make test suite and try to make this project pass the
entire test suite.