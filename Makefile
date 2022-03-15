
build:
	docker buildx build --platform linux/arm64,linux/amd64 -t itaylor/docker-vhoster --push ./