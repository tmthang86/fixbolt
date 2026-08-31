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
#include <string.h>
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

/* The tail, which is the half the median cannot see.
 *
 * `nohz_full` is bought to remove jitter and charged for in median: it costs
 * +155 ns per kernel entry on this machine. Whether that is a good trade is a
 * question about the far tail, so this times every call individually and keeps a
 * histogram.
 *
 * Per-call `clock_gettime` adds a constant ~46 ns to every reading — two vDSO
 * calls at ~23 ns. It is present in BOTH arms and it does not hide excursions,
 * which is what this mode is for. The p50 it reports must equal `syscall_loop`
 * plus that overhead; where it does not, something other than the syscall is
 * being measured and the reading is void.
 *
 * 8 ns buckets, 8192 of them: 32 KB, small enough to stay in L2 so the histogram
 * itself does not generate the misses it is trying to observe. Anything past
 * 65 us lands in the overflow bucket, and the exact maximum is kept separately
 * because the one worst call is the number a latency budget is written against.
 */
#define JBUCKETS 8192
#define JSHIFT 3          /* 8 ns per bucket */

static unsigned int hist[JBUCKETS + 1];

static void jitter(unsigned long n) {
    unsigned long sink = 0;
    double worst = 0.0;
    for (unsigned long i = 0; i < n; i++) {
        double t0 = now_ns();
        sink += (unsigned long)syscall(SYS_getpid);
        double t1 = now_ns();
        double d = t1 - t0;
        if (d > worst) worst = d;
        unsigned long b = (unsigned long)d >> JSHIFT;
        hist[b < JBUCKETS ? b : JBUCKETS]++;
    }

    /* Percentiles straight off the histogram. */
    const double q[] = {0.50, 0.99, 0.999, 0.9999};
    const char *qn[] = {"p50", "p99", "p99.9", "p99.99"};
    printf("  n=%lu", n);
    unsigned long seen = 0, target_i = 0;
    for (unsigned long b = 0; b <= JBUCKETS && target_i < 4; b++) {
        seen += hist[b];
        while (target_i < 4 && (double)seen >= q[target_i] * (double)n) {
            printf("  %s %.0f", qn[target_i], (double)(b << JSHIFT));
            target_i++;
        }
    }
    printf("  max %.0f ns", worst);
    /* How much of the run is in the far tail at all, in absolute calls: a
     * percentile hides whether "the tail" is six calls or sixty thousand. */
    unsigned long over_1us = 0;
    for (unsigned long b = (1000 >> JSHIFT) + 1; b <= JBUCKETS; b++) over_1us += hist[b];
    printf("  over_1us %lu", over_1us);
    if (sink == 0) fprintf(stderr, "impossible: sink is zero\n");
    printf("\n");
}

int main(int argc, char **argv) {
    unsigned long sink = 0;

    if (argc > 1) {
        /* --jitter: the tail mode, run on its own so the histogram is not
         * polluted by the warm-up loops the default mode needs. */
        if (strcmp(argv[1], "--jitter") != 0) {
            fprintf(stderr, "usage: %s [--jitter]\n", argv[0]);
            return 2;
        }
        user_loop(200000000UL);          /* same warm-up, same reason */
        jitter(5000000UL);
        return 0;
    }

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
