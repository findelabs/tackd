# Tackd

## What is Tackd

Tackd is an encrypted message post, which enables parties to anonymously and security transmit and receive data via a RESTful API.

Tackd encrypts payloads with the XChaCha20Poly1305 cipher upon receipt. This data is then persisted in the backing MongoDB database, retrievable by a client with the required decryption key. The encryption key is returned to the original sender, with the key not persisted by Tackd. 

By default, Tackd will persisted messages for one hour, or a single retrieval, whichever comes first. These settings can be overridden be the sender. 

## Tackd API

### File Upload
Upload a file to Tackd.io

**URL** : `/upload`  

**Sample URL**: `https://tackd.io/upload`

**Optional Queries**:
- `expires`: Set data expiration time in seconds. 
- `reads`: Set maximum number of reads for uploaded file.  
- `password`: Lock file with additional user-provided password.  
  
**Method** : `POST`  
  
### Response Codes:  
  
- Success : `200 OK`
- Error: `500 Internal Server Error`  
  
**Sample Response**  
```json  
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

### File Retrieval
Download a file from Tackd.io

**URL** : `/note/<id>`  

**Sample URL's**: 
- `https://tackd.io/note/d2e1152b-ef91-4e4a-834c-62c41a4278e9?key=ldR9aQY5pBZThQtgsvb0YqK9xmerCBN0`  
- `https://tackd.io/note/myfile.txt?id=d2e1152b-ef91-4e4a-834c-62c41a4278e9?key=ldR9aQY5pBZThQtgsvb0YqK9xmerCBN0`  

**Required Queries**
- `key`: Decryption key.  

**Optional Queries**:
- `id`: ID if file being retrieved.  
- `password`: Unlock file with user-specified password.  
  
**Method** : `GET`  
  
### Response Codes:  
  
- Success : `200 OK`
- Error: `401 Not Found`  
- Error: `500 Internal Server Error`  
  
## Limits

- Data max age is currently capped at one month
- Payloads are only accepted up to 200MB
