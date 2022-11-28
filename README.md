# Tackd

## What is Tackd

Tackd is an encrypted message post, which enables parties to anonymously and security transmit and receive data via a RESTful API.

Tackd encrypts payloads with the XChaCha20Poly1305 cipher upon receipt. Indexing data is then persisted in the backing MongoDB database, with the encrypted data stored in Cloud Storage. The encryption key is returned to the original sender, with the key not persisted by Tackd. Data retrieval is possible by any client with the required decryption key, as well as optional password, if it was provided when data was uploaded.  

By default, Tackd will persisted messages for one hour, or a single retrieval, whichever comes first. These settings can be overridden be the sender. 

## Tackd API

### Base URL

All API calls should be directed to either a locally running instance, or to the public `https://tackd.io` server.  

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

### Register User
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
| Success  | 200   | Returns user id        |
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

### Create New API Key
Create new API key for user.

`POST /api/v1/user/apiKeys`

#### Authentication

| Type     | User      | Notes                  |
|----------|-----------|------------------------|
| Basic    | UUID      | Unique User ID         |
| Basic    | API Key   | API Key/Secret         |

#### Response Codes 
| Type     | Code  | Notes                  |
|----------|-------|------------------------|
| Success  | 200   | New API key and secret |
| Error    | 401   | Not Found              |
| Error    | 500   | Internal server error  |

#### Sample Response
```json  
{
  "created": true,
  "data": {
    "key": "CzsIzBHz",
    "secret": "00d757a55081cc58896c",
    "created": "2022-11-28T00:33:24.366572901Z"
  }
}
```

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
| Success  | 200   | New API key and secret |
| Error    | 401   | Unauthorized           |
| Error    | 500   | Internal server error  |

#### Sample Response
```json  
[
  {
    "key": "CzsIzBHz",
    "created": "2022-11-28T00:33:24.366572901Z"
  }
]
```

### Delete API Key
Delete user's API key.  

`DELETE /api/v1/user/apiKeys/{key}`

#### Path Parameters
| Attribute | Type    | Requirement | Notes             |
|-----------|---------|-------------|-------------------|
| key       | string  | required    | Specify API key   |

#### Authentication
| Type     | User      | Notes                  |
|----------|-----------|------------------------|
| Basic    | UUID      | Unique User ID         |
| Basic    | API Key   | API Key/Secret         |

#### Response Codes 
| Type     | Code  | Notes                  |
|----------|-------|------------------------|
| Success  | 200   | Key deleted            |
| Error    | 401   | Unauthorized           |
| Error    | 500   | Internal server error  |

#### Sample Response
```json  
{
  "delete": true
}
```

### List Uploads
List user's uploads.  

`GET /api/v1/user/uploads`

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
    "id": "3f868d3d-3b04-4b6c-a6ce-238093684b52",
    "meta": {
      "content_type": "application/x-www-form-urlencoded",
      "user_agent": "curl/7.84.0",
      "x_forwarded_for": "172.21.116.163",
      "bytes": 44,
      "filename": "test.txt"
    },
    "lifecycle": {
      "max": {
        "reads": 1,
        "seconds": 3600,
        "expires": 1669600896
      },
      "current": {
        "reads": 0
      }
    }
  }
]
```

## Limits

- Data max age is currently capped at one month
- Payloads are only accepted up to 200MB
