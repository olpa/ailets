import asyncio
import sys


want_to_call: list[str] = ["foo", "bar"]  # eventually also "zak"

event_awaker = asyncio.Event()

flag_want_more: bool = False

returned_tasks = set()


def find_next_task() -> str | None:
    global flag_want_more
    if not flag_want_more:
        return None
    flag_want_more = False

    for wtc in want_to_call:
        if wtc not in returned_tasks:
            returned_tasks.add(wtc)
            return wtc

    return None


async def foo():
    global flag_want_more
    for i in range(10):
        print(f"foo {i}")
        await asyncio.sleep(1 / 10)
        if i == 3:
            flag_want_more = True
            event_awaker.set()
    event_awaker.set()


async def bar():
    global flag_want_more
    for i in range(10):
        print(f"bar {i}")
        await asyncio.sleep(1 / 10)
        if i == 6:
            want_to_call.append("zak")
            flag_want_more = True
            event_awaker.set()
    event_awaker.set()


async def zak():
    global flag_want_more
    for i in range(10):
        print(f"zak {i}")
        await asyncio.sleep(1 / 10)
    flag_want_more = True
    event_awaker.set()


current_tasks: set[asyncio.Task] = set()


def add_next_task():
    task_name = find_next_task()
    if task_name is None:
        return
    task = {
        "foo": foo,
        "bar": bar,
        "zak": zak,
    }[task_name]

    task = asyncio.create_task(task(), name=task_name)
    current_tasks.add(task)


async def awaker():
    await event_awaker.wait()


async def runner():
    i = 0
    while len(current_tasks) > 0:
        i += 1
        print(f"{i}: Current tasks: {[t.get_name() for t in current_tasks]}")

        event_awaker.clear()
        awaker_task = asyncio.create_task(awaker(), name="awaker")
        (done, pending) = await asyncio.wait(
            [*current_tasks, awaker_task], return_when=asyncio.FIRST_COMPLETED
        )

        for task in done:
            print(f"{i}: done: {task.get_name()}")
        for task in pending:
            print(f"{i}: pending: {task.get_name()}")

        if not awaker_task.done():
            awaker_task.cancel()
        for task in done:
            remove_task(task)
        add_next_task()


def remove_task(task: asyncio.Task):
    if task in current_tasks:
        current_tasks.remove(task)


async def finish_tasks():
    await asyncio.gather(*current_tasks)


async def seed():
    global flag_want_more
    flag_want_more = True


async def main():
    """Main function that coordinates the iterator and processing"""

    next_task = asyncio.create_task(seed(), name="seed")
    current_tasks.add(next_task)
    await runner()

    await finish_tasks()


if __name__ == "__main__":
    asyncio.run(main())
