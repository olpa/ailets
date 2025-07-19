# Streaming Tool Calls Implementation Plan

## Overview
Transform the current "collect all tool calls then process" approach into a streaming approach that processes tool calls as they appear in the input stream.

## Current State Analysis
- `funcalls.rs`: Contains `FunCalls` struct that manages function call collection with delta-based updates
- `structure_builder.rs`: Handles streaming message building but needs tool call output integration
- Two input modes: "all at once" (`funcall_response.txt`) and "streaming" (`funcall_streaming.txt`)
- Streaming mode uses deltas where only "arguments" can be spread across multiple deltas

## Implementation Steps

### Step 1: Enhance StructureBuilder for Tool Call Output
**Goal**: Add methods to `StructureBuilder` to output tool calls in streaming fashion
**Changes**: ~30-50 lines
- Add `inject_tool_calls()` method to process and output function calls
- Add helper methods for tool call JSON formatting
- Update tests to verify tool call output format

### Step 2: Add Streaming Tool Call Processing Logic
**Goal**: Implement streaming detection and processing in `StructureBuilder`
**Changes**: ~40-60 lines  
- Add state tracking for streaming vs batch mode
- Implement `write_long_bytes()` integration for argument deltas
- Add logic to detect when a tool call is complete and ready for output

### Step 3: Update FunCalls for Streaming Constraints
**Goal**: Add validation for streaming assumptions and error handling
**Changes**: ~20-30 lines
- Add validation that only "arguments" field spans deltas
- Add validation that "index" increments properly (0, 1, 2...)
- Add error reporting for assumption violations

### Step 4: Integrate with Actor System
**Goal**: Update `lib.rs` and `handlers.rs` to use streaming tool calls
**Changes**: ~30-50 lines
- Modify message processing to call tool call streaming methods
- Update handlers to process tool calls as they arrive
- Ensure proper coordination between text and tool call streams

### Step 5: Add Comprehensive Tests
**Goal**: Test streaming assumptions and edge cases
**Changes**: ~50-80 lines
- Test valid streaming sequences
- Test assumption violations (non-argument fields spanning deltas, invalid index sequences)
- Test mixed text and tool call content
- Test error conditions

## Key Requirements
- Only "arguments" can span multiple deltas in streaming mode
- Index values must increment from 0 by exactly 1
- "detach" operation should happen only once for the first tool call
- Changes should be under 100 lines per step
- Each step should be reviewable and committable independently

## Dependencies
- `write_long_bytes` function for processing argument deltas
- Existing `inject_tool_calls` logic pattern
- JSON output format compatibility