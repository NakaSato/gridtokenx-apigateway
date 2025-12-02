# Frontend Implementation Guide: Registration Flow

This guide provides practical implementation examples for integrating the GridTokenX registration flow into a React/Next.js frontend application.

## Table of Contents

1. [Setup & Configuration](#setup--configuration)
2. [API Client Setup](#api-client-setup)
3. [Component Implementation](#component-implementation)
4. [State Management](#state-management)
5. [Error Handling](#error-handling)
6. [Best Practices](#best-practices)

---

## Setup & Configuration

### Install Dependencies

```bash
npm install axios react-hook-form zod @hookform/resolvers
npm install -D @types/node
```

### Environment Variables

Create `.env.local`:

```env
NEXT_PUBLIC_API_URL=http://localhost:8080
NEXT_PUBLIC_APP_URL=http://localhost:3000
```

---

## API Client Setup

### `lib/api-client.ts`

```typescript
import axios, { AxiosError } from "axios";

const API_URL = process.env.NEXT_PUBLIC_API_URL || "http://localhost:8080";

export const apiClient = axios.create({
  baseURL: API_URL,
  headers: {
    "Content-Type": "application/json",
  },
});

// Add JWT token to requests
apiClient.interceptors.request.use((config) => {
  const token = localStorage.getItem("access_token");
  if (token) {
    config.headers.Authorization = `Bearer ${token}`;
  }
  return config;
});

// Handle token expiration
apiClient.interceptors.response.use(
  (response) => response,
  (error: AxiosError) => {
    if (error.response?.status === 401) {
      localStorage.removeItem("access_token");
      window.location.href = "/login";
    }
    return Promise.reject(error);
  }
);

export interface ApiError {
  error: string;
  message: string;
  details?: Record<string, any>;
}

export const handleApiError = (error: unknown): string => {
  if (axios.isAxiosError(error)) {
    const apiError = error.response?.data as ApiError;
    return apiError?.message || "An unexpected error occurred";
  }
  return "An unexpected error occurred";
};
```

---

## Component Implementation

### 1. Registration Form

#### `components/auth/RegisterForm.tsx`

```typescript
"use client";

import { useState } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { apiClient, handleApiError } from "@/lib/api-client";

const registerSchema = z.object({
  username: z.string().min(3).max(50),
  email: z.string().email(),
  password: z.string().min(8).max(128),
  first_name: z.string().min(1).max(100),
  last_name: z.string().min(1).max(100),
});

type RegisterFormData = z.infer<typeof registerSchema>;

interface RegisterResponse {
  message: string;
  email_verification_sent: boolean;
}

export default function RegisterForm() {
  const [isLoading, setIsLoading] = useState(false);
  const [success, setSuccess] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<RegisterFormData>({
    resolver: zodResolver(registerSchema),
  });

  const onSubmit = async (data: RegisterFormData) => {
    setIsLoading(true);
    setError(null);

    try {
      const response = await apiClient.post<RegisterResponse>(
        "/api/auth/register",
        data
      );

      setSuccess(true);
      console.log("Registration successful:", response.data);
    } catch (err) {
      setError(handleApiError(err));
    } finally {
      setIsLoading(false);
    }
  };

  if (success) {
    return (
      <div className="success-message">
        <h2>Registration Successful!</h2>
        <p>Please check your email to verify your account.</p>
        <p className="text-sm text-gray-600">
          We've sent a verification link to your email address.
        </p>
      </div>
    );
  }

  return (
    <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
      <h2 className="text-2xl font-bold">Create Account</h2>

      {error && (
        <div className="error-message bg-red-50 border border-red-200 text-red-700 px-4 py-3 rounded">
          {error}
        </div>
      )}

      <div>
        <label htmlFor="username" className="block text-sm font-medium">
          Username
        </label>
        <input
          {...register("username")}
          type="text"
          id="username"
          className="mt-1 block w-full rounded-md border-gray-300 shadow-sm"
          placeholder="john_doe"
        />
        {errors.username && (
          <p className="text-red-500 text-sm mt-1">{errors.username.message}</p>
        )}
      </div>

      <div>
        <label htmlFor="email" className="block text-sm font-medium">
          Email
        </label>
        <input
          {...register("email")}
          type="email"
          id="email"
          className="mt-1 block w-full rounded-md border-gray-300 shadow-sm"
          placeholder="john.doe@example.com"
        />
        {errors.email && (
          <p className="text-red-500 text-sm mt-1">{errors.email.message}</p>
        )}
      </div>

      <div>
        <label htmlFor="password" className="block text-sm font-medium">
          Password
        </label>
        <input
          {...register("password")}
          type="password"
          id="password"
          className="mt-1 block w-full rounded-md border-gray-300 shadow-sm"
          placeholder="Min. 8 characters"
        />
        {errors.password && (
          <p className="text-red-500 text-sm mt-1">{errors.password.message}</p>
        )}
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div>
          <label htmlFor="first_name" className="block text-sm font-medium">
            First Name
          </label>
          <input
            {...register("first_name")}
            type="text"
            id="first_name"
            className="mt-1 block w-full rounded-md border-gray-300 shadow-sm"
          />
          {errors.first_name && (
            <p className="text-red-500 text-sm mt-1">
              {errors.first_name.message}
            </p>
          )}
        </div>

        <div>
          <label htmlFor="last_name" className="block text-sm font-medium">
            Last Name
          </label>
          <input
            {...register("last_name")}
            type="text"
            id="last_name"
            className="mt-1 block w-full rounded-md border-gray-300 shadow-sm"
          />
          {errors.last_name && (
            <p className="text-red-500 text-sm mt-1">
              {errors.last_name.message}
            </p>
          )}
        </div>
      </div>

      <button
        type="submit"
        disabled={isLoading}
        className="w-full bg-blue-600 text-white py-2 px-4 rounded-md hover:bg-blue-700 disabled:opacity-50"
      >
        {isLoading ? "Creating Account..." : "Sign Up"}
      </button>

      <p className="text-sm text-center text-gray-600">
        Already have an account?{" "}
        <a href="/login" className="text-blue-600 hover:underline">
          Log in
        </a>
      </p>
    </form>
  );
}
```

---

### 2. Email Verification Page

#### `app/verify-email/page.tsx`

```typescript
"use client";

import { useEffect, useState } from "react";
import { useSearchParams, useRouter } from "next/navigation";
import { apiClient, handleApiError } from "@/lib/api-client";

interface VerifyEmailResponse {
  message: string;
  email_verified: boolean;
  verified_at: string;
  wallet_address: string;
  auth?: {
    access_token: string;
    token_type: string;
    expires_in: number;
    user: {
      username: string;
      email: string;
      role: string;
      blockchain_registered: boolean;
    };
  };
}

export default function VerifyEmailPage() {
  const searchParams = useSearchParams();
  const router = useRouter();
  const [status, setStatus] = useState<"loading" | "success" | "error">(
    "loading"
  );
  const [message, setMessage] = useState("");
  const [walletAddress, setWalletAddress] = useState<string | null>(null);

  useEffect(() => {
    const token = searchParams.get("token");

    if (!token) {
      setStatus("error");
      setMessage("Invalid verification link");
      return;
    }

    verifyEmail(token);
  }, [searchParams]);

  const verifyEmail = async (token: string) => {
    try {
      const response = await apiClient.get<VerifyEmailResponse>(
        `/api/auth/verify-email?token=${token}`
      );

      setStatus("success");
      setMessage(response.data.message);
      setWalletAddress(response.data.wallet_address);

      // Auto-login if JWT token is provided
      if (response.data.auth) {
        localStorage.setItem("access_token", response.data.auth.access_token);

        // Redirect to dashboard after 3 seconds
        setTimeout(() => {
          router.push("/dashboard");
        }, 3000);
      }
    } catch (err) {
      setStatus("error");
      setMessage(handleApiError(err));
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-50">
      <div className="max-w-md w-full bg-white shadow-lg rounded-lg p-8">
        {status === "loading" && (
          <div className="text-center">
            <div className="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-600 mx-auto"></div>
            <p className="mt-4 text-gray-600">Verifying your email...</p>
          </div>
        )}

        {status === "success" && (
          <div className="text-center">
            <div className="text-green-600 text-6xl mb-4">✓</div>
            <h2 className="text-2xl font-bold text-gray-900 mb-2">
              Email Verified!
            </h2>
            <p className="text-gray-600 mb-4">{message}</p>

            {walletAddress && (
              <div className="bg-blue-50 border border-blue-200 rounded-lg p-4 mb-4">
                <p className="text-sm font-medium text-blue-900 mb-2">
                  Your Solana Wallet
                </p>
                <p className="text-xs font-mono text-blue-700 break-all">
                  {walletAddress}
                </p>
              </div>
            )}

            <button
              onClick={() => router.push("/login")}
              className="w-full bg-blue-600 text-white py-2 px-4 rounded-md hover:bg-blue-700"
            >
              Continue to Login
            </button>
          </div>
        )}

        {status === "error" && (
          <div className="text-center">
            <div className="text-red-600 text-6xl mb-4">✗</div>
            <h2 className="text-2xl font-bold text-gray-900 mb-2">
              Verification Failed
            </h2>
            <p className="text-gray-600 mb-4">{message}</p>
            <button
              onClick={() => router.push("/resend-verification")}
              className="w-full bg-blue-600 text-white py-2 px-4 rounded-md hover:bg-blue-700"
            >
              Resend Verification Email
            </button>
          </div>
        )}
      </div>
    </div>
  );
}
```

---

### 3. Login Form

#### `components/auth/LoginForm.tsx`

```typescript
"use client";

import { useState } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { useRouter } from "next/navigation";
import { apiClient, handleApiError } from "@/lib/api-client";

const loginSchema = z.object({
  email: z.string().email(),
  password: z.string().min(1, "Password is required"),
});

type LoginFormData = z.infer<typeof loginSchema>;

interface LoginResponse {
  access_token: string;
  token_type: string;
  expires_in: number;
  user: {
    username: string;
    email: string;
    role: string;
    blockchain_registered: boolean;
  };
}

export default function LoginForm() {
  const router = useRouter();
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<LoginFormData>({
    resolver: zodResolver(loginSchema),
  });

  const onSubmit = async (data: LoginFormData) => {
    setIsLoading(true);
    setError(null);

    try {
      const response = await apiClient.post<LoginResponse>(
        "/api/auth/login",
        data
      );

      // Store JWT token
      localStorage.setItem("access_token", response.data.access_token);

      // Store user info
      localStorage.setItem("user", JSON.stringify(response.data.user));

      // Redirect to dashboard
      router.push("/dashboard");
    } catch (err) {
      const errorMessage = handleApiError(err);

      // Check for email verification error
      if (errorMessage.includes("email") && errorMessage.includes("verif")) {
        setError("Please verify your email before logging in.");
      } else {
        setError(errorMessage);
      }
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
      <h2 className="text-2xl font-bold">Log In</h2>

      {error && (
        <div className="bg-red-50 border border-red-200 text-red-700 px-4 py-3 rounded">
          {error}
          {error.includes("verify") && (
            <a
              href="/resend-verification"
              className="block mt-2 text-sm underline"
            >
              Resend verification email
            </a>
          )}
        </div>
      )}

      <div>
        <label htmlFor="email" className="block text-sm font-medium">
          Email
        </label>
        <input
          {...register("email")}
          type="email"
          id="email"
          className="mt-1 block w-full rounded-md border-gray-300 shadow-sm"
        />
        {errors.email && (
          <p className="text-red-500 text-sm mt-1">{errors.email.message}</p>
        )}
      </div>

      <div>
        <label htmlFor="password" className="block text-sm font-medium">
          Password
        </label>
        <input
          {...register("password")}
          type="password"
          id="password"
          className="mt-1 block w-full rounded-md border-gray-300 shadow-sm"
        />
        {errors.password && (
          <p className="text-red-500 text-sm mt-1">{errors.password.message}</p>
        )}
      </div>

      <button
        type="submit"
        disabled={isLoading}
        className="w-full bg-blue-600 text-white py-2 px-4 rounded-md hover:bg-blue-700 disabled:opacity-50"
      >
        {isLoading ? "Logging in..." : "Log In"}
      </button>

      <p className="text-sm text-center text-gray-600">
        Don't have an account?{" "}
        <a href="/register" className="text-blue-600 hover:underline">
          Sign up
        </a>
      </p>
    </form>
  );
}
```

---

### 4. Meter Registration Form

#### `components/meters/RegisterMeterForm.tsx`

```typescript
"use client";

import { useState } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import { apiClient, handleApiError } from "@/lib/api-client";

const meterSchema = z.object({
  meter_serial: z.string().min(1, "Meter serial is required"),
  meter_type: z.enum(["residential", "commercial", "solar", "industrial"]),
  location_address: z.string().min(1, "Location address is required"),
});

type MeterFormData = z.infer<typeof meterSchema>;

interface RegisterMeterResponse {
  meter_id: string;
  meter_serial: string;
  wallet_address: string;
  verification_status: string;
  message: string;
}

export default function RegisterMeterForm() {
  const [isLoading, setIsLoading] = useState(false);
  const [success, setSuccess] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [meterInfo, setMeterInfo] = useState<RegisterMeterResponse | null>(
    null
  );

  const {
    register,
    handleSubmit,
    formState: { errors },
    reset,
  } = useForm<MeterFormData>({
    resolver: zodResolver(meterSchema),
  });

  const onSubmit = async (data: MeterFormData) => {
    setIsLoading(true);
    setError(null);

    try {
      const response = await apiClient.post<RegisterMeterResponse>(
        "/api/user/meters",
        data
      );

      setSuccess(true);
      setMeterInfo(response.data);
      reset();
    } catch (err) {
      const errorMessage = handleApiError(err);

      // Provide helpful error messages
      if (errorMessage.includes("email")) {
        setError("Please verify your email before registering meters.");
      } else if (errorMessage.includes("wallet")) {
        setError(
          "Wallet address is required. Please complete email verification first."
        );
      } else {
        setError(errorMessage);
      }
    } finally {
      setIsLoading(false);
    }
  };

  if (success && meterInfo) {
    return (
      <div className="bg-green-50 border border-green-200 rounded-lg p-6">
        <h3 className="text-lg font-semibold text-green-900 mb-2">
          Meter Registered Successfully!
        </h3>
        <p className="text-green-700 mb-4">{meterInfo.message}</p>

        <div className="space-y-2 text-sm">
          <div>
            <span className="font-medium">Meter ID:</span>{" "}
            <span className="font-mono">{meterInfo.meter_id}</span>
          </div>
          <div>
            <span className="font-medium">Serial:</span>{" "}
            {meterInfo.meter_serial}
          </div>
          <div>
            <span className="font-medium">Status:</span>{" "}
            <span className="inline-block px-2 py-1 bg-yellow-100 text-yellow-800 rounded">
              {meterInfo.verification_status}
            </span>
          </div>
        </div>

        <button
          onClick={() => setSuccess(false)}
          className="mt-4 text-blue-600 hover:underline"
        >
          Register another meter
        </button>
      </div>
    );
  }

  return (
    <form onSubmit={handleSubmit(onSubmit)} className="space-y-4">
      <h3 className="text-xl font-bold">Register Smart Meter</h3>

      {error && (
        <div className="bg-red-50 border border-red-200 text-red-700 px-4 py-3 rounded">
          {error}
        </div>
      )}

      <div>
        <label htmlFor="meter_serial" className="block text-sm font-medium">
          Meter Serial Number
        </label>
        <input
          {...register("meter_serial")}
          type="text"
          id="meter_serial"
          className="mt-1 block w-full rounded-md border-gray-300 shadow-sm"
          placeholder="METER-12345-ABC"
        />
        {errors.meter_serial && (
          <p className="text-red-500 text-sm mt-1">
            {errors.meter_serial.message}
          </p>
        )}
      </div>

      <div>
        <label htmlFor="meter_type" className="block text-sm font-medium">
          Meter Type
        </label>
        <select
          {...register("meter_type")}
          id="meter_type"
          className="mt-1 block w-full rounded-md border-gray-300 shadow-sm"
        >
          <option value="">Select meter type</option>
          <option value="residential">Residential</option>
          <option value="commercial">Commercial</option>
          <option value="solar">Solar</option>
          <option value="industrial">Industrial</option>
        </select>
        {errors.meter_type && (
          <p className="text-red-500 text-sm mt-1">
            {errors.meter_type.message}
          </p>
        )}
      </div>

      <div>
        <label htmlFor="location_address" className="block text-sm font-medium">
          Location Address
        </label>
        <textarea
          {...register("location_address")}
          id="location_address"
          rows={3}
          className="mt-1 block w-full rounded-md border-gray-300 shadow-sm"
          placeholder="123 Main St, City, Country"
        />
        {errors.location_address && (
          <p className="text-red-500 text-sm mt-1">
            {errors.location_address.message}
          </p>
        )}
      </div>

      <button
        type="submit"
        disabled={isLoading}
        className="w-full bg-blue-600 text-white py-2 px-4 rounded-md hover:bg-blue-700 disabled:opacity-50"
      >
        {isLoading ? "Registering..." : "Register Meter"}
      </button>
    </form>
  );
}
```

---

## State Management

### Using Context API for Auth State

#### `contexts/AuthContext.tsx`

```typescript
"use client";

import {
  createContext,
  useContext,
  useState,
  useEffect,
  ReactNode,
} from "react";
import { apiClient } from "@/lib/api-client";

interface User {
  username: string;
  email: string;
  role: string;
  blockchain_registered: boolean;
}

interface AuthContextType {
  user: User | null;
  isAuthenticated: boolean;
  isLoading: boolean;
  login: (token: string, user: User) => void;
  logout: () => void;
  refreshUser: () => Promise<void>;
}

const AuthContext = createContext<AuthContextType | undefined>(undefined);

export function AuthProvider({ children }: { children: ReactNode }) {
  const [user, setUser] = useState<User | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  useEffect(() => {
    // Check for existing session
    const token = localStorage.getItem("access_token");
    const storedUser = localStorage.getItem("user");

    if (token && storedUser) {
      setUser(JSON.parse(storedUser));
    }

    setIsLoading(false);
  }, []);

  const login = (token: string, userData: User) => {
    localStorage.setItem("access_token", token);
    localStorage.setItem("user", JSON.stringify(userData));
    setUser(userData);
  };

  const logout = () => {
    localStorage.removeItem("access_token");
    localStorage.removeItem("user");
    setUser(null);
  };

  const refreshUser = async () => {
    try {
      const response = await apiClient.get("/api/user/profile");
      const userData = response.data;
      localStorage.setItem("user", JSON.stringify(userData));
      setUser(userData);
    } catch (error) {
      console.error("Failed to refresh user:", error);
      logout();
    }
  };

  return (
    <AuthContext.Provider
      value={{
        user,
        isAuthenticated: !!user,
        isLoading,
        login,
        logout,
        refreshUser,
      }}
    >
      {children}
    </AuthContext.Provider>
  );
}

export function useAuth() {
  const context = useContext(AuthContext);
  if (context === undefined) {
    throw new Error("useAuth must be used within an AuthProvider");
  }
  return context;
}
```

---

## Error Handling

### Custom Error Boundary

#### `components/ErrorBoundary.tsx`

```typescript
"use client";

import { Component, ReactNode } from "react";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: any) {
    console.error("Error caught by boundary:", error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      return (
        <div className="min-h-screen flex items-center justify-center bg-gray-50">
          <div className="max-w-md w-full bg-white shadow-lg rounded-lg p-8">
            <h2 className="text-2xl font-bold text-red-600 mb-4">
              Something went wrong
            </h2>
            <p className="text-gray-600 mb-4">
              {this.state.error?.message || "An unexpected error occurred"}
            </p>
            <button
              onClick={() => window.location.reload()}
              className="w-full bg-blue-600 text-white py-2 px-4 rounded-md hover:bg-blue-700"
            >
              Reload Page
            </button>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}
```

---

## Best Practices

### 1. **Protected Routes**

```typescript
// middleware.ts
import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";

export function middleware(request: NextRequest) {
  const token = request.cookies.get("access_token");
  const isAuthPage =
    request.nextUrl.pathname.startsWith("/login") ||
    request.nextUrl.pathname.startsWith("/register");

  if (!token && !isAuthPage) {
    return NextResponse.redirect(new URL("/login", request.url));
  }

  if (token && isAuthPage) {
    return NextResponse.redirect(new URL("/dashboard", request.url));
  }

  return NextResponse.next();
}

export const config = {
  matcher: ["/dashboard/:path*", "/meters/:path*", "/login", "/register"],
};
```

### 2. **Form Validation**

- Use `zod` for schema validation
- Validate on both client and server
- Provide clear error messages
- Show field-level errors

### 3. **Loading States**

- Show loading indicators during API calls
- Disable form submission while loading
- Provide feedback for long operations

### 4. **Error Recovery**

- Provide retry mechanisms
- Show helpful error messages
- Link to relevant help pages
- Log errors for debugging

### 5. **Security**

- Never store sensitive data in localStorage
- Use HTTPS in production
- Implement CSRF protection
- Sanitize user inputs

---

## Complete Flow Example

```typescript
// app/onboarding/page.tsx
"use client";

import { useState } from "react";
import RegisterForm from "@/components/auth/RegisterForm";
import LoginForm from "@/components/auth/LoginForm";
import RegisterMeterForm from "@/components/meters/RegisterMeterForm";
import { useAuth } from "@/contexts/AuthContext";

export default function OnboardingPage() {
  const { isAuthenticated, user } = useAuth();
  const [step, setStep] = useState<"register" | "login" | "meter">("register");

  return (
    <div className="min-h-screen bg-gray-50 py-12">
      <div className="max-w-md mx-auto">
        {/* Progress Indicator */}
        <div className="mb-8">
          <div className="flex justify-between mb-2">
            <span className={step === "register" ? "font-bold" : ""}>
              Sign Up
            </span>
            <span className={step === "login" ? "font-bold" : ""}>Login</span>
            <span className={step === "meter" ? "font-bold" : ""}>
              Register Meter
            </span>
          </div>
          <div className="h-2 bg-gray-200 rounded">
            <div
              className="h-full bg-blue-600 rounded transition-all"
              style={{
                width:
                  step === "register"
                    ? "33%"
                    : step === "login"
                    ? "66%"
                    : "100%",
              }}
            />
          </div>
        </div>

        {/* Forms */}
        <div className="bg-white shadow-lg rounded-lg p-8">
          {step === "register" && <RegisterForm />}
          {step === "login" && <LoginForm />}
          {step === "meter" && isAuthenticated && <RegisterMeterForm />}
        </div>
      </div>
    </div>
  );
}
```

---

**Next Steps**: Implement these components in your frontend application and test the complete registration flow!
