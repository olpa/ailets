lint:
	flake8 --max-line-length=88 code/*py cons/*py cons/nodes/*py
	mypy -m cons
	mypy -m cons.nodes

fix:
	black code/*y cons/*py cons/nodes/*py
