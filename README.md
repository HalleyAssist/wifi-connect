<h1 align="center"><img width="460" src="https://github.com/resin-io/resin-wifi-connect/raw/master/docs/images/wifi-connect.png" /></h1>

> Easy WiFi setup for Linux devices from your mobile phone or laptop

WiFi Connect is a utility for dynamically setting the WiFi configuration on a Linux device via a captive portal. WiFi credentials are specified by connecting with a mobile phone or laptop to the access point that WiFi Connect creates.

[![Current Release](https://img.shields.io/github/release/resin-io/resin-wifi-connect.svg?style=flat-square)](https://github.com/resin-io/resin-wifi-connect/releases/latest)
[![CircleCI status](https://img.shields.io/circleci/project/github/resin-io/resin-wifi-connect.svg?style=flat-square)](https://circleci.com/gh/resin-io/resin-wifi-connect)
[![License](https://img.shields.io/github/license/resin-io/resin-wifi-connect.svg?style=flat-square)](https://github.com/resin-io/resin-wifi-connect/blob/master/LICENSE)
[![Issues](https://img.shields.io/github/issues/resin-io/resin-wifi-connect.svg?style=flat-square)](https://github.com/resin-io/resin-wifi-connect/issues)

<div align="center">
  <sub>an open source :satellite: project by <a href="https://resin.io">resin.io</a></sub>
</div>

***

[**Download**][DOWNLOAD] | [**How it works**](#how-it-works) | [**How to use**](#how-to-use) | [**Support**](#support) | [**Roadmap**][MILESTONES]

[DOWNLOAD]: https://github.com/resin-io/resin-wifi-connect/releases/latest
[MILESTONES]: https://github.com/resin-io/resin-wifi-connect/milestones

![How it works](./docs/images/how-it-works.png?raw=true)

How it works
------------

WiFi Connect interacts with NetworkManager, which should be the active network manager on the device's host OS.

### 1. Advertise: Device Creates Access Point

WiFi Connect detects available WiFi networks and opens an access point with a captive portal. Connecting to this access point with a mobile phone or laptop allows new WiFi credentials to be configured.

### 2. Connect: User Connects Phone to Device Access Point

Connect to the opened access point on the device from your mobile phone or laptop. The access point SSID is, by default, `WiFi Connect`. It can be changed by setting the `--portal-ssid` command line argument or the `PORTAL_SSID` environment variable (see [this guide](https://docs.resin.io/management/env-vars/) for how to manage environment variables when running on top of resinOS). By default, the network is unprotected, but a WPA2 passphrase can be added by setting the `--portal-passphrase` command line argument or the `PORTAL_PASSPHRASE` environment variable.

### 3. Portal: Phone Shows Captive Portal to User

After connecting to the access point from a mobile phone, it will detect the captive portal and open its web page. Opening any web page will redirect to the captive portal as well.

### 4. Credentials: User Enters Local WiFi Network Credentials on Phone

The captive portal provides the option to select a WiFi SSID from a list with detected WiFi networks and enter a passphrase for the desired network.

### 5. Connected!: Device Connects to Local WiFi Network

When the network credentials have been entered, WiFi Connect will disable the access point and try to connect to the network. If the connection fails, it will enable the access point for another attempt. If it succeeds, the configuration will be saved by NetworkManager.

---

For a complete list of command line arguments and environment variables check out our [command line arguments](./docs/command-line-arguments.md) guide.

The full application flow is illustrated in the [state flow diagram](./docs/state-flow-diagram.md).

***

Installation
------------

WiFi Connect is designed to work on systems like Raspbian or Debian, or run in a docker container on top of resinOS.

### Raspbian/Debian Stretch

WiFi Connect depends on NetworkManager, but by default Raspbian Stretch uses dhcpcd as a network manager. The provided installation shell script disables dhcpcd, installs NetworkManager as the active network manager and downloads and installs WiFi Connect.

Run the following in your terminal, then follow the onscreen instructions:

`bash <(curl -L https://github.com/resin-io/resin-wifi-connect/raw/master/scripts/raspbian-install.sh)`

### resinOS

WiFi Connect can be integrated with a [resin.io](http://resin.io) application. (New to resin.io? Check out the [Getting Started Guide](http://docs.resin.io/#/pages/installing/gettingStarted.md).) This integration is accomplished through the use of two shared files:
- The [Dockerfile template](./Dockerfile.template) manages dependencies. The example included here has everything necessary for WiFi Connect. Application dependencies need to be added. For help with Dockerfiles, take a look at this [guide](https://docs.resin.io/deployment/dockerfile/).
- The [start script](./scripts/start.sh) should contain the commands that run the application. Adding these commands at the end of the script will ensure that everything kicks off after WiFi is correctly configured. 
An example of using WiFi Connect in a Python project can be found [here](https://github.com/resin-io-projects/resin-wifi-connect-example).

***

Supported boards / dongles
--------------------------

WiFi Connect has been successfully tested using the following WiFi dongles:

Dongle                                     | Chip
-------------------------------------------|-------------------
[TP-LINK TL-WN722N](http://bit.ly/1P1MdAG) | Atheros AR9271
[ModMyPi](http://bit.ly/1gY3IHF)           | Ralink RT3070
[ThePiHut](http://bit.ly/1LfkCgZ)          | Ralink RT5370

It has also been successfully tested with the onboard WiFi on a Raspberry Pi 3.

Given these results, it is probable that most dongles with *Atheros* or *Ralink* chipsets will work.

The following dongles are known **not** to work (as the driver is not friendly with access point mode or NetworkManager):

* Official Raspberry Pi dongle (BCM43143 chip)
* Addon NWU276 (Mediatek MT7601 chip)
* Edimax (Realtek RTL8188CUS chip)

Dongles with similar chipsets will probably not work.

WiFi Connect is expected to work with all resin.io supported boards as long as they have the compatible dongles.

***

API
---

Endpoints:
 * /connect POST: ssid, identity, passphrase
 * /networks GET
 * /enable_ap GET
 * /disable_ap GET
 * /restart_ap GET # also rescans nearby SSIDs
 * /has_connection GET

By default the pairing code is used for the passphrase, padded with "_" at the start of the string to the minimum of 8 characters.


Support
-------

If you're having any problem, please [raise an issue](https://github.com/resin-io/resin-wifi-connect/issues/new) on GitHub or [contact us](https://resin.io/community/), and the resin.io team will be happy to help.

***

License
-------

WiFi Connect is free software, and may be redistributed under the terms specified in
the [license](https://github.com/resin-io/resin-wifi-connect/blob/master/LICENSE).
