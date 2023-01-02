# Tackd

## What is Tackd

Tackd is an encrypted message relay, which enables parties to anonymously and security transmit and receive data via a RESTful API.

Tackd encrypts payloads with the XChaCha20Poly1305 stream cipher upon receipt. Indexing data is then persisted in the backing MongoDB database, with the encrypted data stored in Cloud Storage. The encryption key is returned to the original sender, with the key not persisted by Tackd. Data retrieval is possible by any client with the required decryption key, as well as optional password, if it was provided when data was uploaded.  

By default, Tackd will persisted messages for one hour, or a single retrieval, whichever comes first. These settings can be overridden be the sender. 

## Tackd API

### Base URL

All API calls should be directed to either a locally running instance, or to the public `https://tackd.io` server.  

---
### Upload
Upload a file to Tackd.io.

`POST /upload`  

#### Query Parameters
| Attribute | Type    | Requirement | Notes                                 |
|-----------|---------|-------------|---------------------------------------|
| expires   | int     | optional    | Set data expiration time in seconds   |
| reads     | int     | optional    | Set maximum number of reads for data  |
| pwd       | string  | optional    | Lock data with additional password    |
| filename  | string  | optional    | Specify filename for upload           |
| tags      | string  | optional    | Comma separated tags                  |
  
#### Response Codes 
| Type     | Code  | Notes                  |
|----------|-------|------------------------|
| Success  | 200   | Returns json object    |
| Error    | 500   | Internal server error  |
  
#### Sample Response
```json  
{
  "message": "Saved",
  "url": "https://tackd.io/download/d2e1152b-ef91-4e4a-834c-62c41a4278e9?key=ldR9aQY5pBZThQtgsvb0YqK9xmerCBN0",
  "data": {
    "id": "d2e1152b-ef91-4e4a-834c-62c41a4278e9",
    "key": "ldR9aQY5pBZThQtgsvb0YqK9xmerCBN0",
    "expires in": 3600,
    "max reads": 1
  }
}
```

---
### File Retrieval
Download a file from Tackd.io.

`GET /download/{id}`  

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
| Error    | 404   | Not Found              |
| Error    | 500   | Internal server error  |

---
### Register New User
Register a new email with Tackd.io.

`POST /api/v1/user`

#### Payload Parameters (JSON)
| Field    | Type    | Notes                  |
|----------|---------|------------------------|
| email    | String  | User's email           |
| pwd      | String  | User's password        |

#### Response Codes 
| Type     | Code  | Notes                  |
|----------|-------|------------------------|
| Success  | 200   | Success                |
| Error    | 409   | Email already exists   |
| Error    | 500   | Internal server error  |

#### Sample Payload
```json
{
  "email": "myemail@gmail.com",
  "pwd": "mypassword"
}
```

#### Sample Response
```json  
{
  "created": true,
  "user id": "37a8a05b-742d-4306-bdd8-9e7c4236d42b"
}
```
---
### Recover User ID
Recover UUID for email from Tackd.io.

`POST /api/v1/user/recover/id`

#### Payload Parameters (JSON)
| Field    | Type    | Notes                  |
|----------|---------|------------------------|
| email    | String  | User's email           |
| pwd      | String  | User's password        |

#### Response Codes 
| Type     | Code  | Notes                  |
|----------|-------|------------------------|
| Success  | 200   | Success                |
| Error    | 401   | Unauthorized           |
| Error    | 500   | Internal server error  |

#### Sample Payload
```json
{
  "email": "myemail@gmail.com",
  "pwd": "mypassword"
}
```

#### Sample Response
```json  
{
  "email": "myemail@gmail.com",
  "user id": "4424e943-64c8-4098-921c-93443815d32e"
}
```

---
### Create New API Key
Create new API key for user.

`POST /api/v1/user/apiKeys`

#### Authentication
| Type     | User      | Notes                  |
|----------|-----------|------------------------|
| Basic    | UUID      | Unique User ID         |
| Basic    | API Key   | API Key/Secret         |

#### Query Parameters
| Attribute | Type    | Requirement | Notes                                 |
|-----------|---------|-------------|---------------------------------------|
| tags      | string  | optional    | Comma separated tags                  |
| role      | string  | optional    | admin or upload, defaults to admin    |

#### Response Codes 
| Type     | Code  | Notes                  |
|----------|-------|------------------------|
| Success  | 200   | Success                |
| Error    | 401   | Not Found              |
| Error    | 500   | Internal server error  |

#### Sample Response
```json  
{
  "created": true,
  "data": {
    "key": "CzsIzBHz",
    "secret": "00d757a55081cc58896c",
    "created": "2022-11-28T00:33:24.366572901Z",
    "access": {
      "role": "upload"
    }
  }
}
```

---
### List API Keys
List API keys for user.

`GET /api/v1/user/apiKeys`

#### Authentication
| Type     | User      | Notes                  |
|----------|-----------|------------------------|
| Basic    | UUID      | Unique User ID         |
| Basic    | API Key   | API Key/Secret         |

#### Response Codes 
| Type     | Code  | Notes                  |
|----------|-------|------------------------|
| Success  | 200   | Success                |
| Error    | 401   | Unauthorized           |
| Error    | 500   | Internal server error  |

#### Sample Response
```json  
[
  {
    "key": "CzsIzBHz",
    "created": "2022-11-28T00:33:24.366572901Z",
    "access": {
      "role": "admin"
    }
  }
]
```

---
### Delete API Key
Delete user's API key.  

`DELETE /api/v1/user/apiKeys/{key}`

#### Authentication
| Type     | User      | Notes                  |
|----------|-----------|------------------------|
| Basic    | UUID      | Unique User ID         |
| Basic    | API Key   | API Key/Secret         |

#### Path Parameters
| Attribute | Type    | Requirement | Notes             |
|-----------|---------|-------------|-------------------|
| key       | string  | required    | Specify API key   |

#### Response Codes 
| Type     | Code  | Notes                  |
|----------|-------|------------------------|
| Success  | 200   | Success                |
| Error    | 401   | Unauthorized           |
| Error    | 500   | Internal server error  |

#### Sample Response
```json  
{
  "delete": true
}
```

---
### List Uploads
List user's uploads.  

`GET /api/v1/uploads`

#### Authentication
| Type     | User      | Notes                  |
|----------|-----------|------------------------|
| Basic    | UUID      | Unique User ID         |
| Basic    | API Key   | API Key/Secret         |

#### Query Parameters
| Attribute | Type    | Requirement | Notes                                          |
|-----------|---------|-------------|------------------------------------------------|
| tags      | string  | optional    | Filter by tags, comma seperated                |

#### Response Codes 
| Type     | Code  | Notes                  |
|----------|-------|------------------------|
| Success  | 200   | Success                |
| Error    | 401   | Unauthorized           |
| Error    | 500   | Internal server error  |

#### Sample Response
```json  
[
  {
    "id": "436bdf7f-6d6e-4d26-8177-364ee5c61dca",
    "meta": {
      "created": "2022-12-06T02:07:57.168752Z",
      "content_type": "application/x-www-form-urlencoded",
      "user_agent": "curl/7.84.0",
      "bytes": 44
    },
    "lifecycle": {
      "max": {
        "reads": -1,
        "seconds": 3600,
        "expires": 1670296077
      },
      "current": {
        "reads": 7
      }
    },
    "links": [
      {
        "id": "95345270-5dfe-4d98-aae0-db6ffc73e21d",
        "created": "2022-12-06T02:07:57.168750Z",
        "reads": 2
      },
      {
        "id": "d44b0655-b8db-4706-ad8b-8186e18f8604",
        "created": "2022-12-06T02:08:30.307616Z",
        "reads": 5
      }
    ]
  }
]
```

---
### Get Upload Info
Get single user upload info.  

`GET /api/v1/uploads/{id}`

#### Authentication
| Type     | User      | Notes                  |
|----------|-----------|------------------------|
| Basic    | UUID      | Unique User ID         |
| Basic    | API Key   | API Key/Secret         |

#### Path Parameters
| Attribute | Type    | Requirement | Notes              |
|-----------|---------|-------------|--------------------|
| id        | string  | required    | Specify upload id  |

#### Response Codes 
| Type     | Code  | Notes                  |
|----------|-------|------------------------|
| Success  | 200   | Success                |
| Error    | 401   | Unauthorized           |
| Error    | 500   | Internal server error  |

#### Sample Response
```json  
[
  {
    "id": "436bdf7f-6d6e-4d26-8177-364ee5c61dca",
    "meta": {
      "created": "2022-12-06T02:07:57.168752Z",
      "content_type": "application/x-www-form-urlencoded",
      "user_agent": "curl/7.84.0",
      "bytes": 44
    },
    "lifecycle": {
      "max": {
        "reads": -1,
        "seconds": 3600,
        "expires": 1670296077
      },
      "current": {
        "reads": 2
      }
    },
    "links": [
      {
        "id": "95345270-5dfe-4d98-aae0-db6ffc73e21d",
        "created": "2022-12-06T02:07:57.168750Z",
        "reads": 2
      }
    ]
  }
]
```

---
### Delete Upload
Delete single user upload.  

`DELETE /api/v1/uploads/{id}`

#### Authentication
| Type     | User      | Notes                  |
|----------|-----------|------------------------|
| Basic    | UUID      | Unique User ID         |
| Basic    | API Key   | API Key/Secret         |

#### Path Parameters
| Attribute | Type    | Requirement | Notes              |
|-----------|---------|-------------|--------------------|
| id        | string  | required    | Specify upload id  |

#### Response Codes 
| Type     | Code  | Notes                  |
|----------|-------|------------------------|
| Success  | 200   | Success                |
| Error    | 401   | Unauthorized           |
| Error    | 500   | Internal server error  |

#### Sample Response
```json  
{
  "deleted": true
}
```

---
### List Upload Links
List upload links.  

`GET /api/v1/uploads/{id}/links`

#### Authentication
| Type     | User      | Notes                  |
|----------|-----------|------------------------|
| Basic    | UUID      | Unique User ID         |
| Basic    | API Key   | API Key/Secret         |

#### Path Parameters
| Attribute | Type    | Requirement | Notes              |
|-----------|---------|-------------|--------------------|
| id        | string  | required    | Specify upload id  |

#### Response Codes 
| Type     | Code  | Notes                  |
|----------|-------|------------------------|
| Success  | 200   | Success                |
| Error    | 401   | Unauthorized           |
| Error    | 500   | Internal server error  |

#### Sample Response
```json  
[
  {
    "id": "9aa8de6b-8b4f-492c-b8b7-cd6356387a3f",
    "created": "2022-12-03T03:06:35.260646162Z"
  }
]
```

---
### Create new upload Link
Create new upload link.  

`POST /api/v1/uploads/{id}/links`

#### Authentication
| Type     | User      | Notes                  |
|----------|-----------|------------------------|
| Basic    | UUID      | Unique User ID         |
| Basic    | API Key   | API Key/Secret         |

#### Path Parameters
| Attribute | Type    | Requirement | Notes              |
|-----------|---------|-------------|--------------------|
| id        | string  | required    | Specify upload id  |

#### Query Parameters
| Attribute | Type    | Requirement | Notes                                 |
|-----------|---------|-------------|---------------------------------------|
| tags      | string  | optional    | Comma separated tags                  |

#### Response Codes 
| Type     | Code  | Notes                  |
|----------|-------|------------------------|
| Success  | 200   | Success                |
| Error    | 401   | Unauthorized           |
| Error    | 500   | Internal server error  |

#### Sample Response
```json  
{
  "created": true,
  "url": "https://tackd.io/download/a1ef26eb-ae9e-4793-855b-ebb00aba048f?key=D1i03EFoDvT15HZNtOCdb03rnBqo5TvQ",
  "data": {
    "id": "a1ef26eb-ae9e-4793-855b-ebb00aba048f",
    "key": "D1i03EFoDvT15HZNtOCdb03rnBqo5TvQ",
    "created": "2022-12-03T15:06:51.003586994Z"
  }
}
```

---
### Delete upload Link
Create new upload link.  

`DELETE /api/v1/uploads/{id}/links/{link}`

#### Authentication
| Type     | User      | Notes                  |
|----------|-----------|------------------------|
| Basic    | UUID      | Unique User ID         |
| Basic    | API Key   | API Key/Secret         |

#### Path Parameters
| Attribute | Type    | Requirement | Notes              |
|-----------|---------|-------------|--------------------|
| id        | string  | required    | Specify upload id  |
| link      | string  | required    | Specify link id    |

#### Response Codes 
| Type     | Code  | Notes                  |
|----------|-------|------------------------|
| Success  | 200   | Success                |
| Error    | 401   | Unauthorized           |
| Error    | 500   | Internal server error  |

#### Sample Response
```json  
{
  "deleted": true
}
```

## Limits

- Data max age is currently capped at one month
- Payloads are only accepted up to 200MB
