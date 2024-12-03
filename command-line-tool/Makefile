lint:
	flake8 --max-line-length=88 *.py
	MYPYPATH=../pylib-v1 mypy --strict *.py

fix:
	black *.py

build-docker-image:
	docker build -t olpa/ailets .

deploy-docker-image:
	@if [ -z "$$AILETS_TAG" ]; then \
		echo "Error: AILETS_TAG environment variable is not set"; \
		echo "Usage: AILETS_TAG=0.1.0 make deploy-docker-image"; \
		exit 1; \
	fi
	docker tag olpa/ailets olpa/ailets:$$AILETS_TAG
	docker tag olpa/ailets olpa/ailets:latest
	docker push olpa/ailets:$$AILETS_TAG
	docker push olpa/ailets:latest
