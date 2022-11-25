# Tackd

## What is Tackd

Tackd is an encrypted message post, which enables parties to anonymously and security transmit and receive data via a RESTful API.

Tackd encrypts payloads with the XChaCha20Poly1305 cipher upon receipt. Indexing data is then persisted in the backing MongoDB database, with the encrypted data stored in Cloud Storage. The encryption key is returned to the original sender, with the key not persisted by Tackd. Data retrieval is possible by any client with the required decryption key, as well as optional password, if it was provided when data was uploaded.  

By default, Tackd will persisted messages for one hour, or a single retrieval, whichever comes first. These settings can be overridden be the sender. 

## Tackd API

### Base URL

All API calls should be directed to either a locally running instance, or to the public `https://tackd.io` server.  

### Upload
Upload a file to Tackd.io

`POST /upload`  

#### **Path Parameters**
None

#### Query Parameters
| Attribute | Type    | Requirement | Notes                                 |
|-----------|---------|-------------|---------------------------------------|
| expires   | int     | optional    | Set data expiration time in seconds   |
| reads     | int     | optional    | Set maximum number of reads for data  |
| pwd       | string  | optional    | Lock data with additional password    |
| filename  | string  | optional    | Specify filename for upload           |
  
#### Response Codes 
| Type     | Code  | Notes                  |
|----------|-------|------------------------|
| Success  | 200   | Returns json object    |
| Error    | 500   | Internal server error  |
  
#### Sample Response
```json  
{
  "message": "Saved",
  "url": "https://tackd.io/note/d2e1152b-ef91-4e4a-834c-62c41a4278e9?key=ldR9aQY5pBZThQtgsvb0YqK9xmerCBN0",
  "data": {
    "id": "d2e1152b-ef91-4e4a-834c-62c41a4278e9",
    "key": "ldR9aQY5pBZThQtgsvb0YqK9xmerCBN0",
    "expires in": 3600,
    "max reads": 1
  }
}
```

### File Retrieval
Download a file from Tackd.io

`GET /download/{id}/{filename}`  

#### Path Parameters
| Attribute | Type    | Requirement | Notes                                 |
|-----------|---------|-------------|---------------------------------------|
| id        | string  | required    | Specify data id or file to download   |

#### Query Parameters
| Attribute | Type    | Requirement | Notes                                          |
|-----------|---------|-------------|------------------------------------------------|
| id        | string  | optional    | ID to get, use if filename is passed in path   |
| key       | string  | required    | Decryption key                                 |
| pwd       | string  | optional    | Unlock data with password                      |
  
#### Response Codes 
| Type     | Code  | Notes                  |
|----------|-------|------------------------|
| Success  | 200   | Returns binary data    |
| Error    | 401   | Not Found              |
| Error    | 500   | Internal server error  |
  
## Limits

- Data max age is currently capped at one month
- Payloads are only accepted up to 200MB
