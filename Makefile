PROJECT ?= 
IMAGE_NAME := cashweb-backends
VERSION := $(shell git rev-parse HEAD)

.PHONY: image push

image:
	docker build . -t $(PROJECT)$(IMAGE_NAME):latest
	docker tag $(PROJECT)$(IMAGE_NAME):latest $(PROJECT)$(IMAGE_NAME):$(VERSION)

push: image
	docker push $(PROJECT)$(IMAGE_NAME):$(VERSION)
	docker push $(PROJECT)$(IMAGE_NAME):latest
