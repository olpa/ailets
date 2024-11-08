lint:
	flake8 --max-line-length=88 ailets/cons/*py ailets/cons/nodes/*py
	mypy -m ailets.cons
	mypy -m ailets.cons.nodes

fix:
	black ailets/cons/*py ailets/cons/nodes/*py

build-docker-image:
	docker build -t ailets-pymodule .