# Moza Native Visible Lane Closeout

Status: blocked; archived from the active goal on 2026-07-18
Lane: `ci/hardware/moza-r5/2026-05-13`
Archived manifest: `.openracing/goals/archive/2026-07-18-moza-native-visible-lane.toml`
Successor active goal: `.openracing/goals/active.toml`

## Verified frontier

The lane remains `native_response_ready`. Native visible motion is not proven:
the controlled-angle and closed-loop attempts recorded safe undertravel, and
the vendor-authority follow-up recorded a regressed PIDFF response. The checked-
in evidence keeps `hardware_output_authorized=false` and
`visible_motion_verified=false`.

The lane is therefore paused for external hardware/protocol evidence, not
declared complete. The detailed operator handoff remains in
`plans/moza-native-visible-lane/handoff.md`.

## Claim boundary

This closeout records source-of-truth handoff only. It does not authorize a
future output attempt, promote native-visible readiness, claim smoke or release
readiness, establish simulator or vendor-app compatibility, or broaden any
hardware/support claim. Future Moza work requires a separately activated goal,
fresh receipt-backed authorization, and the exact hardware evidence gates in
the handoff.

## Archive reason

The external-control-stream lane has a ready next work item and was explicitly
advanced by the maintainer-directed issue-by-issue workflow. Archiving this
manifest avoids multiple active goals while preserving the Moza lane's blocked
state and evidence pointers.
