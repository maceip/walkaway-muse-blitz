# Audit package boundary

Wrapper for shared audit record definitions and receipt verification.

Canonical audit events currently originate in Hermes. Do not create a second
audit writer. A verifier may read Hermes audit records and independently
check proposal, decision, fingerprint, frame, and screenshot-hash bindings.
