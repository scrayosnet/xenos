![The official Logo of Xenos](.github/images/logo.png "Xenos")

![A visual badge for the latest release](https://img.shields.io/github/v/release/scrayosnet/xenos "Latest Release")
![A visual badge for the version](https://img.shields.io/github/go-mod/go-version/scrayosnet/xenos "Go Version")
![A visual badge for the code quality](https://img.shields.io/scrutinizer/quality/g/scrayosnet/xenos "Code Quality")
![A visual badge for the license](https://img.shields.io/github/license/scrayosnet/xenos "License")

Xenos is a Minecraft profile data proxy that can be used as an ultra-fast replacement for the
official [Mojang API][mojang-api-docs] on [Kubernetes][kubernetes].

*The name "Xenos" is derived from the Greek word xénos which means something like "stranger" or "guest
friend" ([source][name-source]). That alludes to the process of profile resolution by which strangers become known
players.*

## Motivation

Mojang already offers an API to query the profile data of Minecraft accounts, but it is strictly rate limited. And while
the performance is overall *okay*, it's by no means optimal and can therefore only be used as a fallback. Xenos aims for
performance and reliability and can be used as the primary source of truth for profile information.

It is built with scaling and high availability in mind and can be set up fairly easy.

## Major Features

* Perform [gRPC][grpc-docs] or [HTTP REST][rest-docs] lookups for profile information.
* Do not worry about rate limits or caching at all! [^1]
* Get best-in-class caching inbuilt and always request Xenos with very low latency.
* Set Xenos up with replication, to get high availability.
* Store resolved information to allow for grace periods for the resolution between name changes!

## Getting started

Once this project is ready, information about how to run Xenos will be published here. Stay tuned!

## Reporting Security Issues

To report a security issue for this project, please note our [Security Policy][security-policy].

## Code of Conduct

Participation in this project comes under the [Contributor Covenant Code of Conduct][code-of-conduct].

## How to contribute

Thanks for considering contributing to this project! In order to submit a Pull Request, please read
our [contributing][contributing-guide] guide. This project is in active development, and we're always happy to receive
new contributions!

## License

This project is developed and distributed under the MIT License. See [this explanation][mit-license-doc] for a rundown
on what that means.

[mojang-api-docs]: https://wiki.vg/Mojang_API

[kubernetes]: https://kubernetes.io/

[name-source]: https://en.wikipedia.org/wiki/Xenos_(Greek)

[grpc-docs]: https://grpc.io/

[rest-docs]: https://en.wikipedia.org/wiki/Representational_state_transfer

[security-policy]: SECURITY.md

[contributing-guide]: CONTRIBUTING.md

[mit-license-doc]: https://choosealicense.com/licenses/mit/

[^1]: Provided you're attaching enough different IP addresses to Xenos to sustain lookup bursts. Xenos will distribute
its requests with Round-Robin until it runs out of tickets. The API is currently limited to 600 requests per 10 minutes.
