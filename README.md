# Tackd

### What is Tackd

Tackd is an encrypted message post, which enables parties to anonymously and security transmit and receive data via a RESTful API.

Tackd encrypts payloads with the XChaCha20Poly1305 cipher upon receipt. This data is then persisted in the backing MongoDB database, retrievable by a client with the required decryption key. The encryption key is returned to the original sender, with the key not persisted by Tackd. 

By default, Tackd will persisted messages for 30 days, or a single retrieval, whichever comes first. These settings can be overridden be the sender. 

### Tackd API

Tackd can be accessed at https://tackd.io. Posts are accepted at `/upload`, and will return a payload like below:

```
{
  "message": "Saved",
  "url": "https://tackd.io/note/d2e1152b-ef91-4e4a-834c-62c41a4278e9?key=ldR9aQY5pBZThQtgsvb0YqK9xmerCBN0",
  "data": {
    "id": "d2e1152b-ef91-4e4a-834c-62c41a4278e9",
    "key": "ldR9aQY5pBZThQtgsvb0YqK9xmerCBN0",
    "expires in": 300,
    "max reads": 1
  }
}
```

The note can then be accessed by any individual with the full url in the return body.

The sender's content-type header will be included in the response to any retriever. Expire time in seconds may be overridden with the query `?expires={{ seconds }}`, and max number of reads with `?reads={{ retrievals }}`.

### Limits

- Data is only persisted for a maximum of one month  
- Payloads are only accepted up to 10MB
