/* Two loops on one core, in one program: one that never enters the kernel, and
 * one that does nothing else. Together they say WHERE the cost of an isolated
 * core lands, on a machine that cannot be rebooted between the two readings.
 *
 * `docs/reference/measured-costs.md` measured DESIGN.md §9's isolated core at
 * +36% on `Engine::turn`, and could not say why. Run this under `taskset` on an
 * isolated core and on an ordinary one:
 *
 *   same `user_loop`, slower `syscall_loop`  -> the cost is kernel entry/exit
 *   both slower by the same ratio            -> the cost is the clock
 *
 * `user_loop` is also the honesty check for every reading. If it differs between
 * two cores, the two cores are not running at the same speed and NOTHING else in
 * that run may be compared. It is what caught `scaling_cur_freq` reporting
 * 2.24 GHz for a core that was demonstrably running at 3.79 — see
 * `scripts/measure-isolation-cost.sh`.
 *
 * No dependencies, no build system: `scripts/measure-isolation-cost.sh` compiles
 * it with cc -O2. It is C rather than a Rust bench because it must run under
 * `taskset` one core at a time with nothing else in the process, and because
 * `benches/` asserts against per-CPU baselines that do not exist for a machine
 * booted into an experimental kernel command line.
 */
#define _GNU_SOURCE
#include <stdio.h>
#include <time.h>
#include <unistd.h>
#include <sys/syscall.h>

/* CLOCK_MONOTONIC through the vDSO: reading the clock does not itself enter the
 * kernel, so the timer is not part of what `syscall_loop` measures. */
static double now_ns(void) {
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return (double)t.tv_sec * 1e9 + (double)t.tv_nsec;
}

/* Pure user space: a dependent multiply-add chain, one register, no memory
 * traffic, no branch that can mispredict, and no way for the kernel to be
 * involved. Its rate is the core's speed and nothing else. */
static unsigned long user_loop(unsigned long n) {
    unsigned long x = 1;
    for (unsigned long i = 0; i < n; i++) {
        x = x * 6364136223846793005UL + 1442695040888963407UL;
    }
    return x;
}

/* Nothing but kernel entry and exit. `syscall(SYS_getpid)` rather than getpid(),
 * because glibc caches the pid and the wrapper would never leave user space —
 * which would make this loop a second, slower `user_loop` and the whole
 * comparison vacuous. */
static unsigned long syscall_loop(unsigned long n) {
    unsigned long s = 0;
    for (unsigned long i = 0; i < n; i++) {
        s += (unsigned long)syscall(SYS_getpid);
    }
    return s;
}

/* Minimum over repetitions, not mean: this asks how fast the core CAN do the
 * work, and every source of noise here only ever adds time. */
static double best_of(int reps, unsigned long n, unsigned long (*f)(unsigned long),
                      unsigned long *sink) {
    double best = 1e18;
    for (int r = 0; r < reps; r++) {
        double t0 = now_ns();
        *sink += f(n);
        double t1 = now_ns();
        double per = (t1 - t0) / (double)n;
        if (per < best) best = per;
    }
    return best;
}

int main(void) {
    unsigned long sink = 0;

    /* Let the core reach whatever frequency it is going to hold before anything
     * is timed. An isolated core has been asleep; the first reading on it would
     * otherwise measure the ramp. */
    sink += user_loop(200000000UL);

    double u = best_of(7, 100000000UL, user_loop, &sink);
    double s = best_of(7, 2000000UL, syscall_loop, &sink);

    printf("user_loop %8.4f ns/iter    syscall_loop %8.2f ns/call\n", u, s);
    /* Keeps both loops from being optimised away without printing on the hot line. */
    if (sink == 0) fprintf(stderr, "impossible: sink is zero\n");
    return 0;
}
