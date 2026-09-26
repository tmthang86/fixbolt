# How-to guides

A how-to guide gets one task done. Most of fixbolt's are sections of four longer pages today: the
[embedding guide](../GUIDE.md), the two operating guides, one per mode, and the Linux tuning
playbook. This index goes from the task to the section.

## Building on the engine

| To… | Read |
|---|---|
| Choose between `standard` and `hft` mode | [GUIDE.md §0](../GUIDE.md#0-first-decide-your-mode) |
| Pick how to write your handler: `Handler`, or `Application` with prebuilt templates | [GUIDE.md §1b](../GUIDE.md#1b-two-ways-to-write-a-handler) |
| Serve many counterparties from one acceptor | [GUIDE.md §1c](../GUIDE.md#1c-many-counterparties-one-registry-and-a-configuration-file) |
| Run many sessions across threads | [GUIDE.md §1a](../GUIDE.md#1a-running-many-sessions-shard-across-threads-do-not-stack-on-one) |
| Keep slow work off the engine thread | [GUIDE.md §2](../GUIDE.md#2-the-engine-calls-you-on-its-hot-path), then [standard mode §4](../best-practices-standard.md#4-what-the-handler-should-and-should-not-do) |
| Read a price as a decimal | [GUIDE.md §3c](../GUIDE.md) |
| Set a session schedule | [GUIDE.md §5a](../GUIDE.md#5a-session-schedules-and-the-timezone-trap) |
| Choose a journal policy | [GUIDE.md §6](../GUIDE.md#6-journalling-pick-the-policy-deliberately), then [standard mode §3](../best-practices-standard.md#3-journal-durability-pick-the-policy-your-recovery-needs) |
| Dial out as an initiator, and reconnect | [GUIDE.md §8c](../GUIDE.md#8c-dialling-out-and-coming-back) |
| Send a message your handler was not asked for | [GUIDE.md §8d](../GUIDE.md#8d-speaking-first-from-an-application-two-doors) |
| Distribute a binary and meet QuickFIX's notice conditions | [GUIDE.md §10](../GUIDE.md#10-distributing-a-binary-carries-a-quickfix-notice-obligation) |

## Operating it

| To… | Read |
|---|---|
| Watch a running engine | [GUIDE.md §8a](../GUIDE.md#8a-watching-a-running-engine) |
| Export metrics to Prometheus | [standard mode §10](../best-practices-standard.md), [`hft` mode §10](../best-practices-hft.md) |
| Stop the engine in order | [standard mode §9](../best-practices-standard.md#9-stopping-it-in-standard-mode), [`hft` mode §8](../best-practices-hft.md#8-stopping-it-in-hft-mode) |
| Turn on the message log | [standard mode §8](../best-practices-standard.md#8-the-message-log), [`hft` mode §7](../best-practices-hft.md#7-the-message-log) |
| Name and pin the cores for `hft` | [`hft` mode §3](../best-practices-hft.md#3-cores-named-by-you-pinned-from-inside-read-back) |
| Run TLS in `hft` mode | [`hft` mode §9](../best-practices-hft.md#9-tls-in-hft-mode-ktls-is-required-not-a-preference) |
| Tune a Linux host: hardware, BIOS, kernel, NIC | [hft-playbook.md §1–§5](../hft-playbook.md) |
| Measure latency on your own machine | [GUIDE.md §8](../GUIDE.md#8-how-to-benchmark-this-engine-without-fooling-yourself), then [hft-playbook.md §6](../hft-playbook.md#6-measure-a-number-you-can-use) |

## Coming next

Three guides arrive with phase 5's custom-dictionary work, once the code they describe is built:

- *Add a custom tag* (`add-a-custom-tag.md`): accept a tag FIX 4.4 does not define, either by
  not validating user-defined fields or by defining it in your own dictionary.
- *Use a venue dictionary* (`use-a-venue-dictionary.md`): a venue's FIX 4.4 dialect as an overlay
  on the shipped dictionary, or as a whole file.
- *Migrate from QuickFIX* (`migrate-from-quickfix.md`): QuickFIX's configuration keys and what each
  becomes here.
