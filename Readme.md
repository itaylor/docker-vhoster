# Docker-vhoster

Listens on the docker socket for containers to start/end and adds entries for them to your `/etc/hosts` file.

* Written in rust
* Published as slim container on dockerhub as `itaylor/docker-vhoster`

This is designed to be used alongside a tool like `jwilder/nginx-proxy` which creates vhost entries for running services.  It allows you to map them to the hosts file.

## Configuration

Three options:
* `HOST_FILE_LOCATION`: default: `/etc/hosts` the path to the etc hosts file to modify.
* `ENV_VAR_NAME`: default: `VIRTUAL_HOST` the name of the environment variable to look at to determine what vhosts to add.
* `VHOST_IP_ADDR` default `127.0.0.1` Use this to provide/override the IP address used to map the vhost entries.

When run as a container, pass these as env vars.  When run as a cli program, they are arguments.

You must use this on a machine/container where `/var/run/docker.sock` is available

Typically, if you're running `docker-vhoster` inside of docker, this is done as:
`docker run -v /var/run/docker.sock:/var/run/docker.sock:ro itaylor/docker-vhoster`
