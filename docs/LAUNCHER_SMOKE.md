# Static launcher smoke checklist

This is the current manual checklist for the packwiz launcher. The reports
in `legacy/` cover a retired architecture and are not evidence for this one.
Record commit, launcher version, OS/architecture, isolated game directory,
pack version and sanitized evidence. All scenarios below are **NOT RUN**
until a tester records an observed result. Do not publish a production
release or change production services just to test this cleanup.

Use a disposable Windows profile and Linux profile where supported, a test
game directory, and an approved test pack/server. Preserve real user data.

| Scenario | Required observation | Result |
| --- | --- | --- |
| Clean preparation | Static metadata loads; Java and installer hashes validate; Fabric/pack/runtime prepare; game starts with the saved local nickname. | NOT RUN |
| Existing installation | Saved directory, RAM choice, nickname, skins and settings remain; a current installation is reused. | NOT RUN |
| Verify and update | Startup checking, explicit Verify files and a test pack update work; cancellation or a bad hash does not install partial artifacts or delete files outside the test game directory. | NOT RUN |
| Network/server failure | Unavailable pack data and an offline game server show real unavailable states, not fake success/counts; retry recovers. | NOT RUN |
| Game lifecycle | Single-instance handling, overlapping-launch protection, hide/restore, pre/post hooks and exit/crash reporting work. | NOT RUN |
| Rooms and Discord | Follow ROOMS-AND-DISCORD.md with two accounts; missing Discord does not prevent launching. | NOT RUN |
| Signed self-update | With an already available approved signed test build, validate signature and install/restart behavior; reject tampering. Do not create a production tag for cleanup validation. | NOT RUN |
| Native Linux rollback | Verify the opt-in marker, native binary checks, atomic replacement and previous-binary recovery in a disposable installation. | NOT RUN |

The launcher source remains private. The separate public repository contains
release assets only. No API/CMS deployment, database account, CMS bearer token
or server-issued launcher ticket is a prerequisite for this checklist.
