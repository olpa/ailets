import asyncio
import sys

event = asyncio.Event()

async def my_iter():
    """Async iterator that yields values and waits for an event before continuing"""
    for i in range(3):  # Yield 5 values as an example
        yield i * 1000  # Yield a tuple of (milliseconds, event)
        await event.wait()  # Wait for the event to be set
        event.clear()  # Reset the event for the next iteration

async def process_value(ms: int):
    """Async function that processes a value, waits, and signals completion"""
    print(f"Processing value with {ms}ms delay")
    event.set()  # Signal that processing is complete
    await asyncio.sleep(ms / 1000)  # Convert ms to seconds
    print(f"Finished processing {ms}ms delay")

async def main():
    """Main function that coordinates the iterator and processing"""
    async for ms in my_iter():
        next_task = asyncio.create_task(process_value(ms + 50))
        await next_task

if __name__ == "__main__":
    asyncio.run(main())
