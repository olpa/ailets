class Seqno:
    def __init__(self):
        self._seqno = 1

    def next_seqno(self) -> int:
        """Get the next sequence number.
        
        Returns:
            The next sequence number
        """
        current = self._seqno
        self._seqno += 1
        return current
