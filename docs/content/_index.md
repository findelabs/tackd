+++
title = "Tackd"
sort_by = "weight"
+++

# What is Tackd

Tackd is an encrypted message relay, which enables parties to anonymously and security transmit and receive data via a RESTful API.

Tackd encrypts payloads with the XChaCha20Poly1305 stream cipher upon receipt. Indexing data is then persisted in the backing MongoDB database, with the encrypted data stored in Cloud Storage. The encryption key is returned to the original sender, with the key not persisted by Tackd. Data retrieval is possible by any client with the required decryption key, as well as optional password, if it was provided when data was uploaded.

# Getting Started

Base URL is `https://tackd.io`

1. Create user account  
```shell
curl https://tackd.io/api/v1/user \
-d '{"email":"myemail@gmail.com","pwd":"mypassword"}' \
-H 'content-type: application/json'
```

2. Generate API key  
```shell
curl -u {{ user id }}:mypassword https://tackd.io/api/v1/user/apiKeys?role=admin -XPOST
```

3. Upload string of data  
```shell
curl -u {{ api key }}:{{ api secret }} https://tackd.io/upload?expires=1d\&tags=key:value \
--data-binary "This is my payload"
```

4. List uploads. 
```shell
curl -u {{ api key }}:{{ api secret }} https://tackd.io/api/v1/uploads
```
