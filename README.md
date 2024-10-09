# p6m Developer CLI

This command line utility provides a number of cross-platform conveniences for local development, and encourages
conventions, standards, and speed.

## Installation

Direct download links can be found [here][p6m binaries azure bin]

### Windows

Only available as a direct download for now.
<!-- Add winget command when appropriate -->

### MacOS / Linux

<!-- @christiannuss-ybor: 2024-10-07: commented out since we don't have the tap set up yet
## Intall using brew

```sh
brew install p6m-dev/tap/p6m
p6m --version
```

Also available through direct download
-->

## Install from source code
```sh
sudo cargo install --force --git ssh://git@github.com/p6m-run/p6m-cli.git --root /usr/local
p6m --version
```

## Post Configuration

Some commands rely on environment variables being set in your shell:

For Artifactory-related commands (`p6m context`), you need the following set:

```shell
ARTIFACTORY_USERNAME         # your e-mail address
ARTIFACTORY_IDENTITY_TOKEN   # Generate an Identity Token in Artifactory ("Edit Settings" menu option)
```

For Github-related command (`p6m repos`), you need the follow set:

```shell 
GITHUB_TOKEN  # Generate a classic Personal Access Token in your Github account
```

## Commands

### Managing Repositories

_Make sure you have configured your `GITHUB_TOKEN` environment variable, before using these commands._

From the root or outside of your local `~/orgs` directory, you can pull all repos from all organizations you have access to:

```shell
p6m repos pull  # Pulls all repos from all organizations 
```

From inside an organization within `~/orgs` (Ex: ~/orgs/p6m-example), you can pull all repos from within that organization:

```shell
p6m repos pull  # Pulls all repos for the organization you are currently in 
```

From any directory, you can specify which organization to pull repos from:

```shell
p6m repos pull --org p6m-example  # Pulls all p6m-example repos to ~/orgs/p6m-example 
```

Pull only new repositories.  Do not pull existing repos:

```shell
p6m repos pull --new  # Only pull new repos 
```

### Changing Contexts

_Make sure you have configured your `ARTIFACTORY_USERNAME` & `ARTIFACTORY_IDENTITY_TOKEN` environment variable, before using these commands._

When changing between organizations, you may need to change local configuration to work specifically with that organization.

For example, you may need to change your `~/.m2/settings.xml` to pull artifacts from your organization.  You can easily do so by executing the following command:

```shell
p6m context # From within an organization within ~/orgs
# or
p6m context --org p6m-example  # From anywhere
```

### Looking up Resources

You can quickly view external resources, such as the current GitHub page for the organization or repository you are currently
in, or viewing the Artifactory repositories for the organization you are currently in.

```shell
# Github
p6m open github
p6m open gh

# Artifactory
p6m open artifactory
p6m open af

#ArgoCD
p6m open argocd
p6m open argo
p6m open acd
```

### Purging Local Caches

```shell
p6m purge ide-files # Removes all IDE files from the current directory, recursively, allowing an IDE reset

p6m purge maven {groupId prefix} # Removes all Java Artifacts for the given groupId prefix
# Ex: p6m purge maven p6m
# Ex: p6m purge maven p6m.platform
```

### Automatic SSO Configuration

You can automate configuration of your AWS SSO profiles and credentials to Kubernetes clusters available to you.

* Aws subcommand requires installation of the [AWS cli](https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html)

* Azure subcommand requires installation of the [Azure CLI](https://learn.microsoft.com/en-us/cli/azure/install-azure-cli)

```shell
p6m sso # Runs both aws and azure subcommands

p6m sso aws # Replaces your ~/.aws/config and updates ~/.kube/config with entries for EKS clusters.

p6m sso azure # updates ~/.kube/config with entries for AKS clusters.
```

[p6m binaries azure bin]: https://naxpublicstuffs.blob.core.windows.net/binaries?comp=list&restype=container
