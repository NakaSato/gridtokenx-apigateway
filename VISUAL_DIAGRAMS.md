# Visual Flow Diagrams

This document contains visual diagrams for the GridTokenX registration flow.

## 1. High-Level Registration Flow

```mermaid
flowchart TD
    Start([User Starts Registration]) --> Register[Sign Up Form]
    Register --> ValidateInput{Input Valid?}
    ValidateInput -->|No| ShowError1[Show Validation Error]
    ShowError1 --> Register
    ValidateInput -->|Yes| CheckDuplicate{User Exists?}
    CheckDuplicate -->|Yes| ShowError2[Show Duplicate Error]
    ShowError2 --> Register
    CheckDuplicate -->|No| CreateUser[Create User Account]
    CreateUser --> GenerateToken[Generate Verification Token]
    GenerateToken --> SendEmail[Send Verification Email]
    SendEmail --> WaitVerify[Wait for Email Verification]

    WaitVerify --> ClickLink[User Clicks Email Link]
    ClickLink --> ValidateToken{Token Valid?}
    ValidateToken -->|No| ShowError3[Show Token Error]
    ShowError3 --> ResendEmail[Resend Verification]
    ResendEmail --> WaitVerify
    ValidateToken -->|Yes| CreateWallet[Generate Solana Wallet]
    CreateWallet --> UpdateUser[Update User: verified=true]
    UpdateUser --> SendWelcome[Send Welcome Email]
    SendWelcome --> ShowSuccess1[Show Success Message]

    ShowSuccess1 --> Login[User Logs In]
    Login --> ValidateCreds{Credentials Valid?}
    ValidateCreds -->|No| ShowError4[Show Login Error]
    ShowError4 --> Login
    ValidateCreds -->|Yes| CheckVerified{Email Verified?}
    CheckVerified -->|No| ShowError5[Email Not Verified]
    ShowError5 --> ResendEmail
    CheckVerified -->|Yes| IssueJWT[Issue JWT Token]
    IssueJWT --> Dashboard[Redirect to Dashboard]

    Dashboard --> RegisterMeter[Register Smart Meter]
    RegisterMeter --> ValidateMeter{Meter Data Valid?}
    ValidateMeter -->|No| ShowError6[Show Meter Error]
    ShowError6 --> RegisterMeter
    ValidateMeter -->|Yes| CheckWallet{Wallet Exists?}
    CheckWallet -->|No| ShowError7[Wallet Required]
    ShowError7 --> Dashboard
    CheckWallet -->|Yes| CreateMeter[Create Meter Record]
    CreateMeter --> SetPending[Set Status: Pending]
    SetPending --> ShowSuccess2[Show Success Message]
    ShowSuccess2 --> Complete([Registration Complete])

    style Start fill:#e1f5e1
    style Complete fill:#e1f5e1
    style ShowError1 fill:#ffe1e1
    style ShowError2 fill:#ffe1e1
    style ShowError3 fill:#ffe1e1
    style ShowError4 fill:#ffe1e1
    style ShowError5 fill:#ffe1e1
    style ShowError6 fill:#ffe1e1
    style ShowError7 fill:#ffe1e1
    style CreateWallet fill:#e1e5ff
    style IssueJWT fill:#e1e5ff
```

---

## 2. State Diagram: User Account States

```mermaid
stateDiagram-v2
    [*] --> Unregistered
    Unregistered --> PendingVerification: Sign Up
    PendingVerification --> Verified: Verify Email
    PendingVerification --> PendingVerification: Resend Email
    Verified --> Authenticated: Login
    Authenticated --> Verified: Logout
    Verified --> Inactive: Admin Deactivate
    Inactive --> Verified: Admin Reactivate
    Authenticated --> MeterRegistered: Register Meter
    MeterRegistered --> Authenticated: Logout
    MeterRegistered --> MeterRegistered: Register More Meters

    note right of PendingVerification
        - email_verified = false
        - wallet_address = NULL
        - Cannot login
    end note

    note right of Verified
        - email_verified = true
        - wallet_address assigned
        - Can login
    end note

    note right of MeterRegistered
        - Has verified email
        - Has wallet
        - Has 1+ meters
        - Ready for trading
    end note
```

---

## 3. Component Architecture

```mermaid
graph TB
    subgraph Frontend["Frontend (React/Next.js)"]
        RegForm[Registration Form]
        VerifyPage[Email Verification Page]
        LoginForm[Login Form]
        MeterForm[Meter Registration Form]
        Dashboard[User Dashboard]
    end

    subgraph API["API Gateway (Rust/Axum)"]
        AuthHandler[Auth Handlers]
        UserHandler[User Management Handlers]
        MeterHandler[Meter Verification Handlers]
        EmailHandler[Email Verification Handlers]
    end

    subgraph Services["Services Layer"]
        PasswordSvc[Password Service]
        JWTSvc[JWT Service]
        EmailSvc[Email Service]
        WalletSvc[Wallet Service]
        MeterSvc[Meter Verification Service]
        AuditSvc[Audit Logger]
    end

    subgraph External["External Systems"]
        DB[(PostgreSQL Database)]
        EmailProvider[Email Provider]
        Blockchain[Solana Blockchain]
    end

    RegForm -->|POST /api/auth/register| AuthHandler
    VerifyPage -->|GET /api/auth/verify-email| EmailHandler
    LoginForm -->|POST /api/auth/login| AuthHandler
    MeterForm -->|POST /api/user/meters| MeterHandler

    AuthHandler --> PasswordSvc
    AuthHandler --> JWTSvc
    AuthHandler --> EmailSvc
    EmailHandler --> WalletSvc
    EmailHandler --> EmailSvc
    MeterHandler --> MeterSvc

    PasswordSvc --> DB
    JWTSvc --> DB
    EmailSvc --> EmailProvider
    WalletSvc --> Blockchain
    MeterSvc --> DB
    AuditSvc --> DB

    style Frontend fill:#e1f5e1
    style API fill:#e1e5ff
    style Services fill:#fff4e1
    style External fill:#ffe1e1
```

---

## 4. Database Schema Relationships

```mermaid
erDiagram
    USERS ||--o{ USER_ACTIVITIES : logs
    USERS ||--o{ METER_REGISTRY : owns
    USERS {
        uuid id PK
        string username UK
        string email UK
        string password_hash
        string role
        string first_name
        string last_name
        boolean email_verified
        timestamp email_verified_at
        string email_verification_token
        timestamp email_verification_expires_at
        string wallet_address UK
        boolean blockchain_registered
        boolean is_active
        timestamp created_at
        timestamp updated_at
    }

    USER_ACTIVITIES {
        uuid id PK
        uuid user_id FK
        string activity_type
        jsonb description
        inet ip_address
        string user_agent
        timestamp created_at
    }

    METER_REGISTRY {
        uuid id PK
        uuid user_id FK
        string meter_serial UK
        string verification_method
        string verification_status
        string manufacturer
        string meter_type
        string location_address
        date installation_date
        string verification_proof
        timestamp verified_at
        uuid verified_by FK
        timestamp created_at
        timestamp updated_at
    }
```

---

## 5. Authentication Flow

```mermaid
sequenceDiagram
    participant U as User
    participant F as Frontend
    participant A as API
    participant D as Database
    participant J as JWT Service

    U->>F: Enter email & password
    F->>A: POST /api/auth/login
    A->>D: Query user by email
    D-->>A: User record
    A->>A: Verify password hash
    A->>A: Check email_verified
    A->>A: Check is_active
    A->>J: Generate JWT token
    J-->>A: JWT token
    A->>D: Log login activity
    A-->>F: JWT + user info
    F->>F: Store token in localStorage
    F-->>U: Redirect to dashboard

    Note over F,A: Subsequent requests include JWT
    F->>A: GET /api/user/profile<br/>Authorization: Bearer {token}
    A->>J: Validate JWT
    J-->>A: Token valid + claims
    A->>D: Fetch user data
    D-->>A: User profile
    A-->>F: User data
    F-->>U: Display profile
```

---

## 6. Email Verification Token Lifecycle

```mermaid
stateDiagram-v2
    [*] --> Generated: User Registers
    Generated --> Sent: Email Sent
    Sent --> Valid: Within 24 hours
    Sent --> Expired: After 24 hours
    Valid --> Used: User Clicks Link
    Expired --> Regenerated: Resend Request
    Regenerated --> Sent: New Email Sent
    Used --> [*]: Token Cleared

    note right of Generated
        Token hashed and
        stored in database
    end note

    note right of Expired
        User must request
        new verification email
    end note

    note right of Used
        Token cleared from DB
        One-time use only
    end note
```

---

## 7. Meter Verification Status Flow

```mermaid
stateDiagram-v2
    [*] --> Pending: User Registers Meter
    Pending --> UnderReview: Admin Reviews
    UnderReview --> Verified: Admin Approves
    UnderReview --> Rejected: Admin Rejects
    Rejected --> Pending: User Resubmits
    Verified --> Suspended: Admin Suspends
    Suspended --> Verified: Admin Reinstates
    Verified --> [*]: Meter Active

    note right of Pending
        Awaiting admin verification
        Cannot submit readings
    end note

    note right of Verified
        Meter approved
        Can submit readings
        Can trade energy
    end note

    note right of Rejected
        Verification failed
        User must fix issues
    end note
```

---

## 8. Error Handling Flow

```mermaid
flowchart TD
    Request[API Request] --> Validate{Validation}
    Validate -->|Pass| Auth{Authentication}
    Validate -->|Fail| Error400[400 Bad Request]

    Auth -->|Pass| Authorize{Authorization}
    Auth -->|Fail| Error401[401 Unauthorized]

    Authorize -->|Pass| Process[Process Request]
    Authorize -->|Fail| Error403[403 Forbidden]

    Process --> DBQuery{Database Query}
    DBQuery -->|Success| Response200[200 OK / 201 Created]
    DBQuery -->|Not Found| Error404[404 Not Found]
    DBQuery -->|Error| Error500[500 Internal Server Error]

    Error400 --> LogError[Log Error]
    Error401 --> LogError
    Error403 --> LogError
    Error404 --> LogError
    Error500 --> LogError

    LogError --> ReturnError[Return Error Response]
    Response200 --> LogSuccess[Log Success]

    style Error400 fill:#ffe1e1
    style Error401 fill:#ffe1e1
    style Error403 fill:#ffe1e1
    style Error404 fill:#ffe1e1
    style Error500 fill:#ffe1e1
    style Response200 fill:#e1f5e1
```

---

## 9. Complete Registration Timeline

```mermaid
gantt
    title Registration Flow Timeline
    dateFormat YYYY-MM-DD HH:mm

    section Sign Up
    Fill form           :a1, 2024-01-01 10:00, 2m
    Submit registration :a2, after a1, 1m
    Receive email       :a3, after a2, 30s

    section Email Verification
    Check email         :b1, 2024-01-01 10:15, 5m
    Click link          :b2, after b1, 30s
    Wallet created      :b3, after b2, 2s

    section Login
    Navigate to login   :c1, 2024-01-01 10:25, 1m
    Enter credentials   :c2, after c1, 1m
    Receive JWT         :c3, after c2, 1s

    section Meter Registration
    Fill meter form     :d1, 2024-01-01 10:30, 3m
    Submit meter        :d2, after d1, 1s
    Meter pending       :d3, after d2, 1s

    section Admin Verification
    Admin reviews       :e1, 2024-01-01 14:00, 1h
    Meter approved      :e2, after e1, 1s
```

---

## 10. Security Layers

```mermaid
graph LR
    subgraph Input["Input Layer"]
        I1[User Input]
    end

    subgraph Validation["Validation Layer"]
        V1[Schema Validation]
        V2[Format Validation]
        V3[Business Rules]
    end

    subgraph Auth["Authentication Layer"]
        A1[Password Verification]
        A2[JWT Validation]
        A3[Email Verification]
    end

    subgraph Authz["Authorization Layer"]
        Z1[Role Check]
        Z2[Resource Ownership]
        Z3[Permission Check]
    end

    subgraph Data["Data Layer"]
        D1[SQL Injection Prevention]
        D2[Parameterized Queries]
        D3[Encrypted Storage]
    end

    I1 --> V1
    V1 --> V2
    V2 --> V3
    V3 --> A1
    A1 --> A2
    A2 --> A3
    A3 --> Z1
    Z1 --> Z2
    Z2 --> Z3
    Z3 --> D1
    D1 --> D2
    D2 --> D3

    style Input fill:#ffe1e1
    style Validation fill:#fff4e1
    style Auth fill:#e1e5ff
    style Authz fill:#e1f5ff
    style Data fill:#e1f5e1
```

---

## Usage

These diagrams can be:

- Embedded in documentation
- Used for presentations
- Shared with stakeholders
- Referenced during development
- Included in API documentation

All diagrams are created using Mermaid syntax and will render automatically in:

- GitHub
- GitLab
- Many documentation platforms
- VS Code (with Mermaid extension)

---

**Last Updated**: 2025-12-01  
**Version**: 1.0
