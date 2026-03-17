# Differences: Original Spec vs Actual Implementation

This document records differences between the original specification documents (`A229-latent-pipe-specification.md`, `HANDOVER-a229-latent-pipes.md`, `PLAN-pipepool-refactoring.md`) and the actual code implementation.

**Per project convention**: In this case, the code wins. The specifications have been updated to match the code.

## PipeAccess enum removed

**Original spec**: Used `PipeAccess` enum with `ExistingOnly` and `OrCreateLatent` variants.

**Actual code**: Uses simple `allow_latent: bool` parameter.

**Rationale** (from PLAN-pipepool-refactoring.md): "Deleted `PipeAccess` enum entirely - replaced with simple `allow_latent: bool` parameter"

## Readers not stored in pool

**Original spec**: Proposed storing readers in a vector: `readers: Vec<(Handle, StdHandle, Reader)>`

**Actual code**: Readers are created on-demand and returned to callers. Only `latent_writers` and `writers` are stored.

**Rationale**: "Didn't need to store readers in a vector - they're created on-demand and returned to callers"

## PipeState moved from Pipe to PipePool

**Original spec**: `Pipe` struct contained `PipeState` enum (Latent/Realized/ClosedWithoutData) with state machine logic.

**Actual code**: Simple `Writer` and `Reader` structs (like master branch). Latent handling is entirely in `PipePool` via `LatentWriter` entries.

**Rationale**: "Reader and Writer become simple, stateless (in terms of latent vs realized). All coordination logic is in one place (PipePool)."

## Attachments spawned on writer realization, not eagerly

**Original spec**: "Attachments spawn immediately on latent pipes" - attachments would spawn and block reading until pipe is realized.

**Actual code**: Attachments spawn when `AttachmentManager::on_writer_realized()` is called, which happens when `touch_writer()` creates a new writer. The reader is obtained with `allow_latent=false` since the writer is guaranteed to exist.

**Rationale**: Simpler control flow. No need for attachments to wait on latent pipes.

## LatentWriter simplified

**Original spec**: `LatentWriter` included `name: String` field.

**Actual code**: `LatentWriter` only has `key`, `state`, and `notify`. The `name` field was unused.

## Environment API simplified

**Original spec**: `Environment` had `attach_stdout()`, `attach_stderr()`, `attach_all_stderr()`, and `resolve()` methods.

**Actual code**: Attachment configuration is done via `AttachmentConfig` passed to `AttachmentManager`. `Environment::attach_stdout()` delegates to `AttachmentConfig`.

## Code reduction

The refactoring deleted 516 lines (888 deletions, 372 insertions) while maintaining all functionality.
