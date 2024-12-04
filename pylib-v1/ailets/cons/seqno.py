class Seqno:
    def __init__(self) -> None:
        self._seqno = 1

    def next_seqno(self) -> int:
        """Get the next sequence number.

        Returns:
            The next sequence number
        """
        current = self._seqno
        self._seqno += 1
        return current

    def at_least(self, seqno: int) -> None:
        if self._seqno < seqno:
            self._seqno = seqno
