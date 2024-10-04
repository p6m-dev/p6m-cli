# Ybor Developer CLI

This command line utility provides a number of cross-platform conveniences for local development, and encourages
conventions, standards, and speed.

## Installation

Direct download links can be found [here][ybor binaries azure bin]

### Windows

Only available as a direct download for now.
<!-- Add winget command when appropriate -->

### MacOS / Linux

## Intall using brew

`brew install ybor-tech/tap/ybor`

Also available through direct download

## Install from source code
`git clone git@github.com:ybor-platform/ybor-cli.git`
`cd ybor-cli`
`cargo install --path .`

## Post Configuration

Some commands rely on environment variables being set in your shell:

For Artifactory-related commands (`ybor context`), you need the following set:

```shell
ARTIFACTORY_USERNAME         # your e-mail address
ARTIFACTORY_IDENTITY_TOKEN   # Generate an Identity Token in Artifactory ("Edit Settings" menu option)
```

For Github-related command (`ybor repos`), you need the follow set:

```shell 
GITHUB_TOKEN  # Generate a classic Personal Access Token in your Github account
```

## Commands

### Managing Repositories

_Make sure you have configured your `GITHUB_TOKEN` environment variable, before using these commands._

From the root or outside of your local `~/orgs` directory, you can pull all repos from all organizations you have access to:

```shell
ybor repos pull  # Pulls all repos from all organizations 
```

From inside an organization within `~/orgs` (Ex: ~/orgs/ybor-example), you can pull all repos from within that organization:

```shell
ybor repos pull  # Pulls all repos for the organization you are currently in 
```

From any directory, you can specify which organization to pull repos from:

```shell
ybor repos pull --org ybor-example  # Pulls all ybor-example repos to ~/orgs/ybor-example 
```

Pull only new repositories.  Do not pull existing repos:

```shell
ybor repos pull --new  # Only pull new repos 
```

### Changing Contexts

_Make sure you have configured your `ARTIFACTORY_USERNAME` & `ARTIFACTORY_IDENTITY_TOKEN` environment variable, before using these commands._

When changing between organizations, you may need to change local configuration to work specifically with that organization.

For example, you may need to change your `~/.m2/settings.xml` to pull artifacts from your organization.  You can easily do so by executing the following command:

```shell
ybor context # From within an organization within ~/orgs
# or
ybor context --org ybor-example  # From anywhere
```

### Looking up Resources

You can quickly view external resources, such as the current GitHub page for the organization or repository you are currently
in, or viewing the Artifactory repositories for the organization you are currently in.

```shell
# Github
ybor open github
ybor open gh

# Artifactory
ybor open artifactory
ybor open af

#ArgoCD
ybor open argocd
ybor open argo
ybor open acd
```

### Purging Local Caches

```shell
ybor purge ide-files # Removes all IDE files from the current directory, recursively, allowing an IDE reset

ybor purge maven {groupId prefix} # Removes all Java Artifacts for the given groupId prefix
# Ex: ybor purge maven ybor
# Ex: ybor purge maven ybor.platform
```

### Automatic SSO Configuration

You can automate configuration of your AWS SSO profiles and credentials to Kubernetes clusters available to you.

* Aws subcommand requires installation of the [AWS cli](https://docs.aws.amazon.com/cli/latest/userguide/getting-started-install.html)

* Azure subcommand requires installation of the [Azure CLI](https://learn.microsoft.com/en-us/cli/azure/install-azure-cli)

```shell
ybor sso # Runs both aws and azure subcommands

ybor sso aws # Replaces your ~/.aws/config and updates ~/.kube/config with entries for EKS clusters.

ybor sso azure # updates ~/.kube/config with entries for AKS clusters.
```

[ybor binaries azure bin]: https://naxpublicstuffs.blob.core.windows.net/binaries?comp=list&restype=container
