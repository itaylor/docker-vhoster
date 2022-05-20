
build:
	docker buildx build --platform linux/arm64,linux/amd64 -t itaylor/docker-vhoster --push ./

build-local:
	docker buildx build --platform linux/amd64 -t itaylor/docker-vhoster --output type=docker ./


setup:
	docker run --rm --privileged multiarch/qemu-user-static --reset -p yes
	docker buildx create --name multiarch --driver docker-container --use
	docker buildx inspect --bootstrap