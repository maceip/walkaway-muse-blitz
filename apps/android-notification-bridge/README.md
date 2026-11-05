# Walkaway Bridge — Android notification bridge

This Android project was generated with:

```sh
android create empty-activity --name="Walkaway Bridge" -o=apps/android-notification-bridge
```

The Android CLI installed on this machine exposes the `empty-activity`
Compose template. No Walkaway notification behavior has been implemented in
this wrapper step.

Intended boundary:

- Receive an approval/action-required event from the trusted host side.
- Use the Android/Meta AI pathway available on the paired phone to surface
  the event toward the glasses.
- Never receive or store the Hermes host token.
- Never approve a browser action by itself; a human decision returns through
  the trusted host boundary.


## Meta DAT status

Walkaway Bridge includes its own minimal Meta Wearables DAT integration in
`WalkawayDat.kt`: initialize after Bluetooth permission, monitor
registration/devices, start registration, create one display session, wait
for session/display `STARTED`, send a two-button card, and return physical
Approve/Reject actions to the laptop bus.

Credentials are read only from ignored `local.properties` keys
`mwdat_application_id` and `mwdat_client_token`; no credential values are
committed. The SDK artifacts come from Meta's GitHub Packages repository and
require `github_token` in the same ignored file.

Verified 2026-11-04: this app reached `registration=REGISTERED`,
`devices=1`, and `connected=Meta RB Display 003F`. Its first display session
then returned `Session ended by device` while the owner was away from the
glasses, so a Walkaway-native physical card is not yet claimed as verified.
