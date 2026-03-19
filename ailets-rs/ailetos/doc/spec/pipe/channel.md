# Pipe: Stream Data Transfer

## non-blocking-write

Write completes even without consumers. Write doesn't block for slow consumers.

## one-to-many

Single producer, multiple independent consumers. Each consumer receives identical data sequence.

## blocking-read

Read waits if no data available but producer still active.

## eof-after-data

Consumer receives EOF only after all written data is read.

## error-after-data

Consumer receives writer error only after all written data is read.

## reader-error-immediate

Reader's own error is immediate, regardless of data availability.
