import asyncio
import sys


want_to_call: list[str] = ["foo", "bar"]  # eventually also "zak"

event = asyncio.Event()

async def my_iter():
    called = set()
    for wtc in want_to_call:
        if wtc in called:
            continue
        called.add(wtc)
        yield wtc
        await event.wait()
        event.clear()


async def foo():
    for i in range(10):
        print(f"foo {i}")
        await asyncio.sleep(1 / 10)
        if i == 3:
            event.set()

async def bar():
    for i in range(10):
        print(f"bar {i}")
        await asyncio.sleep(1 / 10)
        if i == 6:
            want_to_call.append("zak")
            event.set()

async def zak():
    for i in range(10):
        print(f"zak {i}")
        await asyncio.sleep(1 / 10)
    event.set()

current_tasks: set[asyncio.Task] = set()

async def awaker():
    await event.wait()

async def add_task(hint: str, task: asyncio.Task):
    current_tasks.add(task)
    while len(current_tasks) > 0:
        awaker_task = asyncio.create_task(awaker())
        (done, pending) = await asyncio.wait(
            [*current_tasks, awaker_task],
            return_when=asyncio.FIRST_COMPLETED
        )
        for task in done:
            print(f"done {hint}: {task.get_name()}")
        for task in pending:
            print(f"pending {hint}: {task.get_name()}")
        if not awaker_task.done():
            awaker_task.cancel()
        for task in done:
            remove_task(task)

def remove_task(task: asyncio.Task):
    if task in current_tasks:
        current_tasks.remove(task)

async def finish_tasks():
    await asyncio.gather(*current_tasks)

async def main():
    """Main function that coordinates the iterator and processing"""
    async for tname in my_iter():
        if tname == "foo":
            next_task = asyncio.create_task(foo(), name=f"foo")
        elif tname == "bar":
            next_task = asyncio.create_task(bar(), name=f"bar")
        elif tname == "zak":
            next_task = asyncio.create_task(zak(), name=f"zak")
        else:
            raise ValueError(f"Unknown task name: {tname}")
        await add_task(tname, next_task)
    await finish_tasks()

if __name__ == "__main__":
    asyncio.run(main())
