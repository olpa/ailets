lint:
	flake8 --max-line-length=88 ailets/cons/*py ailets/cons/nodes/*/*py ailets/tools/*/*py ailets/models/*/*py
	mypy -p ailets

fix:
	black ailets/cons/*py ailets/cons/nodes/*/*py ailets/tools/*/*py ailets/models/*/*py

build-docker-image:
	docker build -t ailets-pymodule .
