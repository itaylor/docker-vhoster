# Docker-vhoster

Listens on the docker socket for containers to start/end and adds entries for them to your `/etc/hosts` file.

This is designed to be used alongside a tool like `jwilder/nginx-proxy` which creates vhost entries for running services.  
If you follow it's pattern for setting `VIRTUAL_HOST` environment variables that program nginx, this will allow those same variables to also control your `/etc/hosts` file.

## One time setup:

We need to give your user permission to modify the /etc/hosts file.  This is done differently on different OS.

On MacOS, run in terminal:
```sh
chmod +a user:$(whoami) allow read,write,append,readattr,writeattr,readextattr,writeextattr,readsecurity /etc/hosts
```

On Windows... Powershell is too complex for me to understand how to do it as a script, so in the UI:
* Navigate in Explorer to `C:\Windows\System32\drivers\etc`
* Right click the `hosts` file -> Properties
* Security -> Edit 
* Add...
* Enter your user name 
* Check Names 
* Ok
* Check "Full Control" box
* Ok, Ok

On Linux: 
Do nothing, this just works

## Options

Options:
All options can be set as cli arguments or as env vars

| argument                           | env var              | default       | description |
| ---------------------------------- | -------------------- | ------------- | ----------- |
| `-h, --host-file-location <path>`  | `HOST_FILE_LOCATION` | `/etc/hosts`  | The path to the hosts file to modify |
| `-e, --env-var-name <string>`      | `ENV_VAR_NAME`       | `VIRTUAL_HOST`| The env var used to look up the host name on a per-container basis | 
| `-v, --vhost-ip-addr <ip address>` | `VHOST_IP_ADDR`      | `127.0.0.1`   | The IP address to set in the /etc/hosts file |

## Required file/volume mount!
You must use this on a machine/container where `/var/run/docker.sock` is available

## Running it
The most common way this is meant to be used is to run inside of docker alongside containers.
Typically, if you're running `docker-vhoster` inside of docker, this is done as:
`docker run -v /var/run/docker.sock:/var/run/docker.sock:ro -v /etc/hosts:/tmp/hosts itaylor/docker-vhoster -h /tmp/hosts`

## Example using docker-compose
See the `example/` folder.
```sh
cd example/
docker-compose up
```
Once the containers start you'll be able to open a browser to `http://web-example.fake.com` and have it display a hello world message.

*Note* if using Docker for windows, you'll have to change the `example/docker-compose.yml` to have the correct path to Windows' `etc/hosts` file.
