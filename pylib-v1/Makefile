lint:
	flake8 --max-line-length=88 cons/*py cons/nodes/*py
	mypy -m cons
	mypy -m cons.nodes

fix:
	black cons/*py cons/nodes/*py
