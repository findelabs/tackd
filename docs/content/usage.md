+++
title = "Usage"
description = "Usage"
weight = 1
+++

# Installation

Currently, installation is done through cargo: 

```shell
cargo install --git https://github.com/findelabs/tackd.git
```

# Usage

Tackd utilizes MongoDB to store the metadata for all objects uploaded through the service. To run Tackd locally, you will need a connection to a MongoDB cluster, along with permissions to a database, which defaults to `tackd`.  

For object storage, Tackd supports either Google Cloud Storage, or Azure Blob as destinations. Azure connections are done through an Azure storage account and an Azure access key. Connections to Google Cloud Storage is done through the `SERVICE_ACCOUNT_JSON` environment variable, which accepts a service account json string.  

```
USAGE:
    tackd [OPTIONS] --mongo <mongo> --bucket <bucket> --keys <keys>

OPTIONS:
    -a, --admin <admin>
            MongoDB Admin Collection [env: TACKD_MONGODB_ADMIN_COLLECTION=] [default: admin]

    -A, --azure_storage_account <azure_storage_account>
            Set Azure Storage Account [env: AZURE_STORAGE_ACCOUNT=]

    -b, --bucket <bucket>
            Bucket name [env: TACKD_BUCKET=]

    -c, --collection <collection>
            MongoDB Metadata Collection [env: TACKD_MONGODB_COLLECTION=] [default: uploads]

    -d, --database <database>
            MongoDB Database [env: TACKD_MONGODB_DATABASE=] [default: tackd]

    -e, --encrypt_data
            Encrypt data before committing to object storage [env: TACKD_ENCRYPT_DATA=]

    -h, --help
            Print help information

    -i, --ignore_link_key
            Ignore link keys, useful for private deployments [env: TACKD_IGNORE_LINK_KEY=]

    -k, --keys <keys>
            Set encryption keys [env: TACKD_KEYS=]

    -l, --limit <limit>
            Set the max payload size in bytes [env: TACKD_UPLOAD_LIMIT=] [default: 10485760]

    -m, --mongo <mongo>
            MongoDB connection url [env: TACKD_MONGODB_URL=]

    -p, --port <port>
            Set port to listen on [env: TACKD_PORT=] [default: 8080]

    -r, --retention <retention>
            Set the default retention ms [env: TACKD_RETENTION_MS=] [default: 3600]

    -R, --reads <reads>
            Set the default read count [env: TACKD_READS=] [default: -1]

    -s, --azure_storage_access_key <azure_storage_access_key>
            Set Azure Storage Access Key [env: AZURE_STORAGE_ACCESS_KEY=]

    -u, --url <url>
            Declare url [env: TACKD_EXTERNAL_URL=] [default: http://localhost:8080]

    -U, --users <users>
            MongoDB Users Collection [env: TACKD_MONGODB_USERS_COLLECTION=] [default: users]

    -V, --version
            Print version information
```
