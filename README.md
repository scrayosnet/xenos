![The official Logo of Xenos](.github/images/logo.png "Xenos")

![A visual badge for the latest release](https://img.shields.io/github/v/release/scrayosnet/xenos "Latest Release")
![A visual badge for the workflow status](https://img.shields.io/github/actions/workflow/status/scrayosnet/xenos/docker.yaml "Workflow Status")
![A visual badge for the dependency status](https://img.shields.io/librariesio/github/scrayosnet/xenos "Dependencies")
![A visual badge for the Docker image size](https://img.shields.io/docker/image-size/scrayosug/xenos "Image Size")
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

It is built with scaling and high availability in mind and can be set up fairly easy. Compared to existing solutions
like [MineTools][minetools-docs], Xenos introduces little to no additional latency and primarily uses cached and
latency-optimized responses to communicate with the services. It is meant to completely replace any kind of internal
caching and always use the API instead for inter process communication.

The differences between Xenos and serverless solutions like [Crafthead][crafthead-docs] are, that Xenos is deployed on
your own infrastructure to minimize latency, Xenos uses your own IPs regarding the rate-limit and you'll have more
over what requests are performed, that Xenos is based on gRPC to cut down API response times even further and that
Xenos will always try to return a value, even if only outdated data is available.

Therefore, Xenos is a reliable service, offering high-availability for Mojang API calls. It offers all the standard
Mojang APIs and will even be able to proxy authentication requests in the future.

## Major Features

* Perform [gRPC][grpc-docs] and [HTTP REST][rest-docs] lookups for profile information.
* Do not worry about rate limits or caching at all! [^1]
* Get best-in-class caching inbuilt and always request Xenos with very low latency.
* Set Xenos up with replication, to get high availability.
* Supply data in any representation without converting it first (dashed vs non-dashed UUIDs).
* Fall back to retrieving cached data, if the Mojang API is currently not available.
* Store resolved information to allow for grace periods for the resolution between name changes!

## Getting started

> [!WARNING]
> Xenos is under active development and may experience breaking changes until the first version is released. After that
> version, breaking changes will be performed in adherence to [Semantic Versioning][semver-docs]

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

[minetools-docs]: https://api.minetools.eu/

[crafthead-docs]: https://crafthead.net/

[grpc-docs]: https://grpc.io/

[rest-docs]: https://en.wikipedia.org/wiki/Representational_state_transfer

[semver-docs]: https://semver.org/lang/de/

[security-policy]: SECURITY.md

[code-of-conduct]: CODE_OF_CONDUCT.md

[contributing-guide]: CONTRIBUTING.md

[mit-license-doc]: https://choosealicense.com/licenses/mit/

[^1]: Provided you're attaching enough different IP addresses to Xenos to sustain lookup bursts. Xenos will distribute
its requests with Round-Robin until it runs out of tickets. The API is currently limited to 600 requests per 10 minutes.
