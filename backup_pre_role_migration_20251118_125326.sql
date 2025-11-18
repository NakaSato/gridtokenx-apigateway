--
-- PostgreSQL database dump
--

\restrict ZvrIBqD36RU79rQo48bQIrYdxpm5PlF34Q38UukoAqi09pq1n4HmdKKlCrcRfQr

-- Dumped from database version 16.11 (Debian 16.11-1.pgdg13+1)
-- Dumped by pg_dump version 16.11 (Debian 16.11-1.pgdg13+1)

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: pgcrypto; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;


--
-- Name: EXTENSION pgcrypto; Type: COMMENT; Schema: -; Owner: 
--

COMMENT ON EXTENSION pgcrypto IS 'cryptographic functions';


--
-- Name: uuid-ossp; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS "uuid-ossp" WITH SCHEMA public;


--
-- Name: EXTENSION "uuid-ossp"; Type: COMMENT; Schema: -; Owner: 
--

COMMENT ON EXTENSION "uuid-ossp" IS 'generate universally unique identifiers (UUIDs)';


--
-- Name: order_side; Type: TYPE; Schema: public; Owner: gridtokenx_user
--

CREATE TYPE public.order_side AS ENUM (
    'buy',
    'sell'
);


ALTER TYPE public.order_side OWNER TO gridtokenx_user;

--
-- Name: order_status; Type: TYPE; Schema: public; Owner: gridtokenx_user
--

CREATE TYPE public.order_status AS ENUM (
    'pending',
    'active',
    'partially_filled',
    'filled',
    'settled',
    'cancelled',
    'expired',
    'pending_cleanup'
);


ALTER TYPE public.order_status OWNER TO gridtokenx_user;

--
-- Name: user_role; Type: TYPE; Schema: public; Owner: gridtokenx_user
--

CREATE TYPE public.user_role AS ENUM (
    'user',
    'admin',
    'ami',
    'producer',
    'consumer'
);


ALTER TYPE public.user_role OWNER TO gridtokenx_user;

--
-- Name: update_updated_at_column(); Type: FUNCTION; Schema: public; Owner: gridtokenx_user
--

CREATE FUNCTION public.update_updated_at_column() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$;


ALTER FUNCTION public.update_updated_at_column() OWNER TO gridtokenx_user;

SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: _sqlx_migrations; Type: TABLE; Schema: public; Owner: gridtokenx_user
--

CREATE TABLE public._sqlx_migrations (
    version bigint NOT NULL,
    description text NOT NULL,
    installed_on timestamp with time zone DEFAULT now() NOT NULL,
    success boolean NOT NULL,
    checksum bytea NOT NULL,
    execution_time bigint NOT NULL
);


ALTER TABLE public._sqlx_migrations OWNER TO gridtokenx_user;

--
-- Name: audit_logs; Type: TABLE; Schema: public; Owner: gridtokenx_user
--

CREATE TABLE public.audit_logs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    event_type character varying(50) NOT NULL,
    user_id uuid,
    ip_address inet,
    event_data jsonb,
    created_at timestamp with time zone DEFAULT now()
);


ALTER TABLE public.audit_logs OWNER TO gridtokenx_user;

--
-- Name: blockchain_transactions; Type: TABLE; Schema: public; Owner: gridtokenx_user
--

CREATE TABLE public.blockchain_transactions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    signature character varying(88) NOT NULL,
    user_id uuid,
    program_id character varying(44) NOT NULL,
    instruction_name character varying(100),
    status character varying(20) DEFAULT 'pending'::character varying NOT NULL,
    fee bigint,
    compute_units_consumed integer,
    submitted_at timestamp with time zone DEFAULT now(),
    confirmed_at timestamp with time zone,
    error_message text,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    CONSTRAINT chk_blockchain_status CHECK (((status)::text = ANY ((ARRAY['pending'::character varying, 'confirmed'::character varying, 'failed'::character varying])::text[])))
);


ALTER TABLE public.blockchain_transactions OWNER TO gridtokenx_user;

--
-- Name: erc_certificate_transfers; Type: TABLE; Schema: public; Owner: gridtokenx_user
--

CREATE TABLE public.erc_certificate_transfers (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    certificate_id uuid NOT NULL,
    from_user_id uuid,
    to_user_id uuid NOT NULL,
    transfer_date timestamp with time zone DEFAULT now() NOT NULL,
    transaction_hash character varying(88),
    created_at timestamp with time zone DEFAULT now(),
    from_wallet character varying(88),
    to_wallet character varying(88) NOT NULL,
    blockchain_tx_signature character varying(88)
);


ALTER TABLE public.erc_certificate_transfers OWNER TO gridtokenx_user;

--
-- Name: erc_certificates; Type: TABLE; Schema: public; Owner: gridtokenx_user
--

CREATE TABLE public.erc_certificates (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    certificate_id character varying(100) NOT NULL,
    wallet_address character varying(88) NOT NULL,
    energy_amount numeric(20,8) NOT NULL,
    certificate_type character varying(20) NOT NULL,
    issuance_date timestamp with time zone NOT NULL,
    expiry_date timestamp with time zone,
    status character varying(20) DEFAULT 'active'::character varying NOT NULL,
    metadata jsonb,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    user_id uuid,
    energy_source character varying(50),
    vintage_year integer,
    kwh_amount numeric(20,8),
    issue_date timestamp with time zone,
    issuer_wallet character varying(88),
    blockchain_tx_signature character varying(88),
    CONSTRAINT chk_cert_status CHECK (((status)::text = ANY ((ARRAY['active'::character varying, 'retired'::character varying, 'expired'::character varying, 'transferred'::character varying])::text[]))),
    CONSTRAINT chk_cert_type CHECK (((certificate_type)::text = ANY ((ARRAY['REC'::character varying, 'ERC'::character varying, 'IREC'::character varying])::text[])))
);


ALTER TABLE public.erc_certificates OWNER TO gridtokenx_user;

--
-- Name: market_epochs; Type: TABLE; Schema: public; Owner: gridtokenx_user
--

CREATE TABLE public.market_epochs (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    epoch_number bigint NOT NULL,
    start_time timestamp with time zone NOT NULL,
    end_time timestamp with time zone NOT NULL,
    status character varying(20) NOT NULL,
    clearing_price numeric(20,8),
    total_volume numeric(20,8) DEFAULT 0,
    total_orders bigint DEFAULT 0,
    matched_orders bigint DEFAULT 0,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    CONSTRAINT chk_epoch_status CHECK (((status)::text = ANY ((ARRAY['pending'::character varying, 'active'::character varying, 'cleared'::character varying, 'settled'::character varying])::text[])))
);


ALTER TABLE public.market_epochs OWNER TO gridtokenx_user;

--
-- Name: meter_readings; Type: TABLE; Schema: public; Owner: gridtokenx_user
--

CREATE TABLE public.meter_readings (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    meter_id character varying(50) NOT NULL,
    wallet_address character varying(88) NOT NULL,
    "timestamp" timestamp with time zone NOT NULL,
    energy_generated numeric(12,4),
    energy_consumed numeric(12,4),
    surplus_energy numeric(12,4),
    deficit_energy numeric(12,4),
    battery_level numeric(5,2),
    temperature numeric(5,2),
    voltage numeric(8,2),
    current numeric(8,2),
    created_at timestamp with time zone DEFAULT now(),
    user_id uuid,
    kwh_amount numeric(12,4),
    minted boolean DEFAULT false,
    mint_signature character varying(88),
    mint_tx_signature character varying(88),
    reading_timestamp timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    submitted_at timestamp with time zone DEFAULT now()
);


ALTER TABLE public.meter_readings OWNER TO gridtokenx_user;

--
-- Name: order_matches; Type: TABLE; Schema: public; Owner: gridtokenx_user
--

CREATE TABLE public.order_matches (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    epoch_id uuid NOT NULL,
    buy_order_id uuid NOT NULL,
    sell_order_id uuid NOT NULL,
    matched_amount numeric(20,8) NOT NULL,
    match_price numeric(20,8) NOT NULL,
    match_time timestamp with time zone DEFAULT now(),
    status character varying(20) DEFAULT 'pending'::character varying NOT NULL,
    settlement_id uuid,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    CONSTRAINT chk_match_status CHECK (((status)::text = ANY ((ARRAY['pending'::character varying, 'settled'::character varying, 'failed'::character varying])::text[]))),
    CONSTRAINT chk_matched_amount CHECK ((matched_amount > (0)::numeric))
);


ALTER TABLE public.order_matches OWNER TO gridtokenx_user;

--
-- Name: settlements; Type: TABLE; Schema: public; Owner: gridtokenx_user
--

CREATE TABLE public.settlements (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    epoch_id uuid NOT NULL,
    buyer_id uuid NOT NULL,
    seller_id uuid NOT NULL,
    energy_amount numeric(20,8) NOT NULL,
    price_per_kwh numeric(20,8) NOT NULL,
    total_amount numeric(20,8) NOT NULL,
    fee_amount numeric(20,8) DEFAULT 0 NOT NULL,
    net_amount numeric(20,8) NOT NULL,
    status character varying(20) DEFAULT 'pending'::character varying NOT NULL,
    transaction_hash character varying(66),
    processed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    CONSTRAINT chk_settlement_status CHECK (((status)::text = ANY ((ARRAY['pending'::character varying, 'processing'::character varying, 'completed'::character varying, 'failed'::character varying])::text[])))
);


ALTER TABLE public.settlements OWNER TO gridtokenx_user;

--
-- Name: trades; Type: VIEW; Schema: public; Owner: gridtokenx_user
--

CREATE VIEW public.trades AS
 SELECT id,
    epoch_id,
    buy_order_id AS buyer_id,
    sell_order_id AS seller_id,
    matched_amount AS quantity,
    match_price AS price,
    match_time AS executed_at,
    (matched_amount * match_price) AS total_value
   FROM public.order_matches;


ALTER VIEW public.trades OWNER TO gridtokenx_user;

--
-- Name: trading_orders; Type: TABLE; Schema: public; Owner: gridtokenx_user
--

CREATE TABLE public.trading_orders (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid NOT NULL,
    epoch_id uuid,
    order_type character varying(10) NOT NULL,
    energy_amount numeric(20,8) NOT NULL,
    price_per_kwh numeric(20,8) NOT NULL,
    filled_amount numeric(20,8) DEFAULT 0,
    transaction_hash character varying(66),
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    settled_at timestamp with time zone,
    kwh_amount numeric(20,8),
    expires_at timestamp with time zone,
    status public.order_status DEFAULT 'pending'::public.order_status,
    side public.order_side,
    CONSTRAINT chk_energy_amount CHECK ((energy_amount > (0)::numeric)),
    CONSTRAINT chk_price CHECK ((price_per_kwh > (0)::numeric))
);


ALTER TABLE public.trading_orders OWNER TO gridtokenx_user;

--
-- Name: user_activities; Type: TABLE; Schema: public; Owner: gridtokenx_user
--

CREATE TABLE public.user_activities (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    user_id uuid,
    activity_type character varying(50) NOT NULL,
    description text,
    ip_address inet,
    user_agent text,
    metadata jsonb,
    created_at timestamp with time zone DEFAULT now()
);


ALTER TABLE public.user_activities OWNER TO gridtokenx_user;

--
-- Name: users; Type: TABLE; Schema: public; Owner: gridtokenx_user
--

CREATE TABLE public.users (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    email character varying(255) NOT NULL,
    username character varying(50) NOT NULL,
    password_hash character varying(255) NOT NULL,
    wallet_address character varying(88),
    role character varying(20) DEFAULT 'user'::character varying NOT NULL,
    user_type character varying(20),
    first_name character varying(100),
    last_name character varying(100),
    is_active boolean DEFAULT true,
    registered_at timestamp with time zone DEFAULT now(),
    created_at timestamp with time zone DEFAULT now(),
    updated_at timestamp with time zone DEFAULT now(),
    email_verified boolean DEFAULT false NOT NULL,
    email_verification_token character varying(128),
    email_verification_sent_at timestamp with time zone,
    email_verification_expires_at timestamp with time zone,
    email_verified_at timestamp with time zone,
    blockchain_registered boolean DEFAULT false
);


ALTER TABLE public.users OWNER TO gridtokenx_user;

--
-- Data for Name: _sqlx_migrations; Type: TABLE DATA; Schema: public; Owner: gridtokenx_user
--

COPY public._sqlx_migrations (version, description, installed_on, success, checksum, execution_time) FROM stdin;
20241101000001	initial schema	2025-11-17 17:38:49.705159+00	t	\\x3a00de13b6434067139030726d6a9bf96de9d5ab23989d3d04fd3657be47dc9aaf6aa36d070f6d9e88e1357584992eeb	45122958
20241102000002	add email verification	2025-11-17 17:38:49.751023+00	t	\\x0625a75e4d16e57a2b7a36edfc2ff54ff76415ccc5a2dc1e165393deb19fb1b01fa5e026d3c5ae599948e8b7ff3a3bfb	3414791
20241114000003	schema fixes	2025-11-17 17:40:43.889479+00	t	\\x1df6715e7972d034717101e4bf61af872411433a089ac92ba9eedabd5554a84f8bd10fab1e8c65c19fa42b2160fd0069	11480542
20241118000004	add missing tables	2025-11-17 18:05:17.305018+00	t	\\x3d397a4baef0aefc642da4cd3673614bfb557d66bb35cf89c9ef85815630396a86479f67dfd8e74e194c19bad7df319a	19126375
20241118000005	add mint tx signature	2025-11-17 18:36:05.788305+00	t	\\xf0854fee0951e7d6e527e61ce393c3d366ea16c6070414ae79b28bb1cf8d47cae970e568cf424238764e3bc61eac6247	8248083
20241118000006	fix schema mismatches	2025-11-17 18:40:01.348112+00	t	\\x8229b9b1be1829d196df3098b206f3da34218305480f94c5be7dab01e02cc8b94579c59b1784eaf16a0d2ff4be5768d0	20023625
20241118000007	add final columns	2025-11-17 18:47:15.60928+00	t	\\x29bb7fa9304a956a4c57a1a332cba2b02ca60eea0c8f645059ed9bcde16d2b6ce8e9f3bf1c309072bbba5d5ffde852bd	23360959
20241118000008	add issuer wallet	2025-11-17 18:50:46.103301+00	t	\\x57f000388dadef408f1960b91021ff9ac5ae2435e07177b757cf2f45f3c65db64263eb979c4b742b3d8ba9eb6900b46d	7697375
20241118100001	convert to enums	2025-11-18 02:16:09.55241+00	t	\\xd6068e3bd66ffd2fede9387057ec1551344930fa3448f1c571f194d026271ab11852277e3a5f444b785fab5996842a09	25647708
20241118100002	add blockchain tx signature	2025-11-18 02:16:09.578889+00	t	\\x842daa34e79cd9cb4c4c751583c7921484309e1951eb3b4b381383beca31b8e556da3a0f6249f7b373d44fd66e7187af	1139792
\.


--
-- Data for Name: audit_logs; Type: TABLE DATA; Schema: public; Owner: gridtokenx_user
--

COPY public.audit_logs (id, event_type, user_id, ip_address, event_data, created_at) FROM stdin;
ea83e9c7-e5b5-4f88-8ec6-c71514d2d485	user_login	3cd3ac81-480b-4ca5-b8a3-ed39505b9011	\N	{"ip": "unknown", "type": "user_login", "user_id": "3cd3ac81-480b-4ca5-b8a3-ed39505b9011", "user_agent": "curl/8.7.1"}	2025-11-18 04:48:07.714945+00
03bef0cf-3124-4340-880a-c0bb0e5afd34	login_failed	\N	\N	{"ip": "unknown", "type": "login_failed", "email": "admin@gridtokenx.com", "reason": "Invalid password", "user_agent": "curl/8.7.1"}	2025-11-18 04:49:01.287492+00
badd228b-6f9e-4378-bbb8-b629d74a33d4	login_failed	\N	\N	{"ip": "unknown", "type": "login_failed", "email": "admin@gridtokenx.com", "reason": "Invalid password", "user_agent": "curl/8.7.1"}	2025-11-18 04:49:21.803787+00
76a0c7c7-98ed-47bd-82f0-60a0341b0c3a	user_login	3cd3ac81-480b-4ca5-b8a3-ed39505b9011	\N	{"ip": "unknown", "type": "user_login", "user_id": "3cd3ac81-480b-4ca5-b8a3-ed39505b9011", "user_agent": "curl/8.7.1"}	2025-11-18 04:49:39.531228+00
422669f7-f6ec-42b4-b929-e7325ea06402	user_login	3cd3ac81-480b-4ca5-b8a3-ed39505b9011	\N	{"ip": "unknown", "type": "user_login", "user_id": "3cd3ac81-480b-4ca5-b8a3-ed39505b9011", "user_agent": "curl/8.7.1"}	2025-11-18 04:49:53.18691+00
46d7e55f-7053-40b9-8597-36d5e359d5b9	user_login	c25f33d4-9cf4-454d-bd5a-68f5217ceb0b	\N	{"ip": "unknown", "type": "user_login", "user_id": "c25f33d4-9cf4-454d-bd5a-68f5217ceb0b", "user_agent": "curl/8.7.1"}	2025-11-18 05:02:00.598449+00
a08f4aa3-4da4-4ab3-9232-341aecebf841	user_login	3e2165a5-f2ca-4b3b-aca5-92f6edff1c72	\N	{"ip": "unknown", "type": "user_login", "user_id": "3e2165a5-f2ca-4b3b-aca5-92f6edff1c72", "user_agent": "curl/8.7.1"}	2025-11-18 05:02:32.548913+00
45c6d7b9-4f23-4649-87f4-4587cbc16768	user_login	a2ab8829-ada0-4440-8ea4-82048cd2b261	\N	{"ip": "unknown", "type": "user_login", "user_id": "a2ab8829-ada0-4440-8ea4-82048cd2b261", "user_agent": "curl/8.7.1"}	2025-11-18 05:03:24.468618+00
2d66120f-ed4d-4943-b640-d14be4339d99	user_login	5073df62-0a32-4556-a463-d9dbbd967a09	\N	{"ip": "unknown", "type": "user_login", "user_id": "5073df62-0a32-4556-a463-d9dbbd967a09", "user_agent": "curl/8.7.1"}	2025-11-18 05:03:30.256118+00
333927e4-2665-4f67-98db-3fbd049dc905	user_login	8022c90b-55a9-4dcc-b06f-b36d6fdd83ec	\N	{"ip": "unknown", "type": "user_login", "user_id": "8022c90b-55a9-4dcc-b06f-b36d6fdd83ec", "user_agent": "curl/8.7.1"}	2025-11-18 05:04:08.023329+00
5489e2db-5eda-4890-bea3-e299c410c4bf	user_login	4ac6d1ab-7131-4637-8c0d-21e259248970	\N	{"ip": "unknown", "type": "user_login", "user_id": "4ac6d1ab-7131-4637-8c0d-21e259248970", "user_agent": "curl/8.7.1"}	2025-11-18 05:04:13.793456+00
d8ddfd08-19ec-46f6-a0cd-5ac99bae212d	user_login	b7ac2b2c-00b6-42b0-bc9d-30a69defcd72	\N	{"ip": "unknown", "type": "user_login", "user_id": "b7ac2b2c-00b6-42b0-bc9d-30a69defcd72", "user_agent": "curl/8.7.1"}	2025-11-18 05:04:54.91728+00
6facfa04-c5d0-4d04-81bb-559ecd4a3877	user_login	80efad2f-057e-45c3-a508-0b0653c8fb95	\N	{"ip": "unknown", "type": "user_login", "user_id": "80efad2f-057e-45c3-a508-0b0653c8fb95", "user_agent": "curl/8.7.1"}	2025-11-18 05:05:00.700996+00
2c069d86-2012-4fa8-8941-0d44025a57a0	user_login	b7ac2b2c-00b6-42b0-bc9d-30a69defcd72	\N	{"ip": "unknown", "type": "user_login", "user_id": "b7ac2b2c-00b6-42b0-bc9d-30a69defcd72", "user_agent": "curl/8.7.1"}	2025-11-18 05:05:55.052986+00
\.


--
-- Data for Name: blockchain_transactions; Type: TABLE DATA; Schema: public; Owner: gridtokenx_user
--

COPY public.blockchain_transactions (id, signature, user_id, program_id, instruction_name, status, fee, compute_units_consumed, submitted_at, confirmed_at, error_message, created_at, updated_at) FROM stdin;
\.


--
-- Data for Name: erc_certificate_transfers; Type: TABLE DATA; Schema: public; Owner: gridtokenx_user
--

COPY public.erc_certificate_transfers (id, certificate_id, from_user_id, to_user_id, transfer_date, transaction_hash, created_at, from_wallet, to_wallet, blockchain_tx_signature) FROM stdin;
\.


--
-- Data for Name: erc_certificates; Type: TABLE DATA; Schema: public; Owner: gridtokenx_user
--

COPY public.erc_certificates (id, certificate_id, wallet_address, energy_amount, certificate_type, issuance_date, expiry_date, status, metadata, created_at, updated_at, user_id, energy_source, vintage_year, kwh_amount, issue_date, issuer_wallet, blockchain_tx_signature) FROM stdin;
\.


--
-- Data for Name: market_epochs; Type: TABLE DATA; Schema: public; Owner: gridtokenx_user
--

COPY public.market_epochs (id, epoch_number, start_time, end_time, status, clearing_price, total_volume, total_orders, matched_orders, created_at, updated_at) FROM stdin;
0ae86628-8f62-4744-ab33-776a57a31448	202511172015	2025-11-17 20:15:00+00	2025-11-17 20:30:00+00	active	\N	0.00000000	0	0	2025-11-17 20:08:55.001426+00	2025-11-17 20:15:54.985443+00
699709fd-2dd7-4493-a70a-b86316c3654d	202511172030	2025-11-17 20:30:00+00	2025-11-17 20:45:00+00	active	\N	0.00000000	0	0	2025-11-17 20:15:54.997606+00	2025-11-17 20:32:12.425618+00
591a4ae7-2ae1-442d-9bd5-4a08a86f9c45	202511180245	2025-11-18 02:45:00+00	2025-11-18 03:00:00+00	pending	\N	0.00000000	0	0	2025-11-18 02:43:07.594964+00	2025-11-18 02:43:07.594964+00
d2843bf0-de45-468f-87a1-7ebddbd54574	202511180315	2025-11-18 03:15:00+00	2025-11-18 03:30:00+00	pending	\N	0.00000000	0	0	2025-11-18 03:01:00.048951+00	2025-11-18 03:01:00.048951+00
\.


--
-- Data for Name: meter_readings; Type: TABLE DATA; Schema: public; Owner: gridtokenx_user
--

COPY public.meter_readings (id, meter_id, wallet_address, "timestamp", energy_generated, energy_consumed, surplus_energy, deficit_energy, battery_level, temperature, voltage, current, created_at, user_id, kwh_amount, minted, mint_signature, mint_tx_signature, reading_timestamp, updated_at, submitted_at) FROM stdin;
\.


--
-- Data for Name: order_matches; Type: TABLE DATA; Schema: public; Owner: gridtokenx_user
--

COPY public.order_matches (id, epoch_id, buy_order_id, sell_order_id, matched_amount, match_price, match_time, status, settlement_id, created_at, updated_at) FROM stdin;
\.


--
-- Data for Name: settlements; Type: TABLE DATA; Schema: public; Owner: gridtokenx_user
--

COPY public.settlements (id, epoch_id, buyer_id, seller_id, energy_amount, price_per_kwh, total_amount, fee_amount, net_amount, status, transaction_hash, processed_at, created_at, updated_at) FROM stdin;
\.


--
-- Data for Name: trading_orders; Type: TABLE DATA; Schema: public; Owner: gridtokenx_user
--

COPY public.trading_orders (id, user_id, epoch_id, order_type, energy_amount, price_per_kwh, filled_amount, transaction_hash, created_at, updated_at, settled_at, kwh_amount, expires_at, status, side) FROM stdin;
4cb9f653-cd60-422d-aacb-8c6f29dda719	80efad2f-057e-45c3-a508-0b0653c8fb95	\N	sell	100.00000000	0.15000000	0.00000000	\N	2025-11-18 05:05:02.740479+00	2025-11-18 05:05:02.740479+00	\N	\N	\N	pending	\N
dd2b8105-19a2-4996-8daf-98136da45bca	b7ac2b2c-00b6-42b0-bc9d-30a69defcd72	\N	buy	50.00000000	0.16000000	0.00000000	\N	2025-11-18 05:05:04.795204+00	2025-11-18 05:05:04.795204+00	\N	\N	\N	pending	\N
37f00231-5075-4c78-9cd3-15a99cf9ce27	b7ac2b2c-00b6-42b0-bc9d-30a69defcd72	\N	buy	50.00000000	0.15000000	0.00000000	\N	2025-11-18 05:06:12.873918+00	2025-11-18 05:06:12.873918+00	\N	\N	\N	pending	\N
\.


--
-- Data for Name: user_activities; Type: TABLE DATA; Schema: public; Owner: gridtokenx_user
--

COPY public.user_activities (id, user_id, activity_type, description, ip_address, user_agent, metadata, created_at) FROM stdin;
\.


--
-- Data for Name: users; Type: TABLE DATA; Schema: public; Owner: gridtokenx_user
--

COPY public.users (id, email, username, password_hash, wallet_address, role, user_type, first_name, last_name, is_active, registered_at, created_at, updated_at, email_verified, email_verification_token, email_verification_sent_at, email_verification_expires_at, email_verified_at, blockchain_registered) FROM stdin;
3cd3ac81-480b-4ca5-b8a3-ed39505b9011	admin@gridtokenx.com	admin	$2b$12$0m11kXoBgc4vvTQE5/eC4O95MnvNYg1vSnH7q8NIfg1L8Fxh8vtFG	\N	admin	\N	System	Admin	t	2025-11-18 04:47:04.243736+00	2025-11-18 04:47:04.243736+00	2025-11-18 04:49:14.335145+00	t	2a1886db4a5fa3ea00d63472d0c9f4d6accd1ee9a2ddad6fc53ed4fda4872384	2025-11-18 04:47:04.251223+00	2025-11-19 04:47:04.251239+00	\N	f
c25f33d4-9cf4-454d-bd5a-68f5217ceb0b	buyer_1763442084@test.com	buyer_1763442084	$2b$12$HdM6xb2UNNVAvdSl0C45/e9sOwew6/zjJZmWZWNnTXclWssaubhMm	\N	consumer	\N	Test	Buyer	t	2025-11-18 05:01:25.746913+00	2025-11-18 05:01:25.746913+00	2025-11-18 05:02:24.604514+00	t	28de778ef4d2062e8c1e957d60b2c3ae6e52e6f847c6899c4b2d7cd8b8853be0	2025-11-18 05:01:25.749576+00	2025-11-19 05:01:25.749406+00	\N	f
3e2165a5-f2ca-4b3b-aca5-92f6edff1c72	buyer_1763442148@test.com	buyer_1763442148	$2b$12$qPUUC8pe6ItWDwJJWBKOFOpGzMwoXydZSC3hbhwJv6u5j9jkg0Qnm	\N	consumer	\N	Test	Buyer	t	2025-11-18 05:02:29.676814+00	2025-11-18 05:02:29.676814+00	2025-11-18 05:02:29.679173+00	f	7eba8ca094deda21538ff9dea8842df010c4a29503d69f6be861df50e41f4ce7	2025-11-18 05:02:29.679173+00	2025-11-19 05:02:29.679073+00	\N	f
a2ab8829-ada0-4440-8ea4-82048cd2b261	buyer_1763442200@test.com	buyer_1763442200	$2b$12$F21Y.wkcIe6EPQT42uc0CeQ5u5zAi6of.EsVVN7n/kFq40wgr/BNe	\N	consumer	\N	Test	Buyer	t	2025-11-18 05:03:21.581614+00	2025-11-18 05:03:21.581614+00	2025-11-18 05:03:21.58414+00	f	dd8c37abbf4b011a10dada440cdf9a2cde0725ea8c842903ed4a9131e68ed6d9	2025-11-18 05:03:21.58414+00	2025-11-19 05:03:21.584328+00	\N	f
5073df62-0a32-4556-a463-d9dbbd967a09	seller_1763442200@test.com	seller_1763442200	$2b$12$3FK3jGHtJJ.SJqC4Cgc.NOrtiRTvq0e4t0akHqMQae.uJsaxznrPO	\N	producer	\N	Test	Seller	t	2025-11-18 05:03:27.32023+00	2025-11-18 05:03:27.32023+00	2025-11-18 05:03:27.323947+00	f	a0b186e015673bd70b094351c919b43263368964d257af0ac4eb3460b9ab91bd	2025-11-18 05:03:27.323947+00	2025-11-19 05:03:27.323657+00	\N	f
8022c90b-55a9-4dcc-b06f-b36d6fdd83ec	buyer_1763442244@test.com	buyer_1763442244	$2b$12$sxa5oAP8qIaqkaNQRUVPUO/bq16aWNaOXL28rdWXd2KjNVDtwQHIK	\N	consumer	\N	Test	Buyer	t	2025-11-18 05:04:05.144546+00	2025-11-18 05:04:05.144546+00	2025-11-18 05:04:05.149061+00	f	5a4826ce2d2ebf470b0faef9287e66dd55a19ce89dbfd29ec1fffe6282055175	2025-11-18 05:04:05.149061+00	2025-11-19 05:04:05.148347+00	\N	f
4ac6d1ab-7131-4637-8c0d-21e259248970	seller_1763442244@test.com	seller_1763442244	$2b$12$bGWgBjuvxOFkirJiyle5VOAPs.NMeg41qkExY5uMaGp3cRimIWCNy	\N	producer	\N	Test	Seller	t	2025-11-18 05:04:10.917822+00	2025-11-18 05:04:10.917822+00	2025-11-18 05:04:10.920244+00	f	795a9d13681936a3c8f458b97bf64789ab76b6356f4675e7ec04e707d468dd3f	2025-11-18 05:04:10.920244+00	2025-11-19 05:04:10.920471+00	\N	f
b7ac2b2c-00b6-42b0-bc9d-30a69defcd72	buyer_1763442291@test.com	buyer_1763442291	$2b$12$CP1R6ztKAmIZBonilzZrG..zirD.epSOK3cuDtdFww6ypQF1BvNJa	\N	consumer	\N	Test	Buyer	t	2025-11-18 05:04:52.061204+00	2025-11-18 05:04:52.061204+00	2025-11-18 05:04:52.06494+00	f	939df8cd3ad1c7c5a90b92cce213dde0c6ccf45d76830c39c95830762cfc7732	2025-11-18 05:04:52.06494+00	2025-11-19 05:04:52.065388+00	\N	f
80efad2f-057e-45c3-a508-0b0653c8fb95	seller_1763442291@test.com	seller_1763442291	$2b$12$XL7kXxaHfNACInQ/0lZP3u4CJvlmkGiYj9ZPp0./v033DDubUuiuK	\N	producer	\N	Test	Seller	t	2025-11-18 05:04:57.821781+00	2025-11-18 05:04:57.821781+00	2025-11-18 05:04:57.82402+00	f	47da2b94e4a39d1dbda40f36facdc58a045972c41dfeaf989deccf34a485503f	2025-11-18 05:04:57.82402+00	2025-11-19 05:04:57.824824+00	\N	f
\.


--
-- Name: _sqlx_migrations _sqlx_migrations_pkey; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public._sqlx_migrations
    ADD CONSTRAINT _sqlx_migrations_pkey PRIMARY KEY (version);


--
-- Name: audit_logs audit_logs_pkey; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.audit_logs
    ADD CONSTRAINT audit_logs_pkey PRIMARY KEY (id);


--
-- Name: blockchain_transactions blockchain_transactions_pkey; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.blockchain_transactions
    ADD CONSTRAINT blockchain_transactions_pkey PRIMARY KEY (id);


--
-- Name: blockchain_transactions blockchain_transactions_signature_key; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.blockchain_transactions
    ADD CONSTRAINT blockchain_transactions_signature_key UNIQUE (signature);


--
-- Name: erc_certificates energy_certificates_certificate_id_key; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.erc_certificates
    ADD CONSTRAINT energy_certificates_certificate_id_key UNIQUE (certificate_id);


--
-- Name: erc_certificates energy_certificates_pkey; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.erc_certificates
    ADD CONSTRAINT energy_certificates_pkey PRIMARY KEY (id);


--
-- Name: erc_certificate_transfers erc_certificate_transfers_pkey; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.erc_certificate_transfers
    ADD CONSTRAINT erc_certificate_transfers_pkey PRIMARY KEY (id);


--
-- Name: market_epochs market_epochs_epoch_number_key; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.market_epochs
    ADD CONSTRAINT market_epochs_epoch_number_key UNIQUE (epoch_number);


--
-- Name: market_epochs market_epochs_pkey; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.market_epochs
    ADD CONSTRAINT market_epochs_pkey PRIMARY KEY (id);


--
-- Name: meter_readings meter_readings_pkey; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.meter_readings
    ADD CONSTRAINT meter_readings_pkey PRIMARY KEY (id);


--
-- Name: order_matches order_matches_pkey; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.order_matches
    ADD CONSTRAINT order_matches_pkey PRIMARY KEY (id);


--
-- Name: settlements settlements_pkey; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.settlements
    ADD CONSTRAINT settlements_pkey PRIMARY KEY (id);


--
-- Name: trading_orders trading_orders_pkey; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.trading_orders
    ADD CONSTRAINT trading_orders_pkey PRIMARY KEY (id);


--
-- Name: user_activities user_activities_pkey; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.user_activities
    ADD CONSTRAINT user_activities_pkey PRIMARY KEY (id);


--
-- Name: users users_email_key; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_email_key UNIQUE (email);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- Name: users users_username_key; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_username_key UNIQUE (username);


--
-- Name: users users_wallet_address_key; Type: CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_wallet_address_key UNIQUE (wallet_address);


--
-- Name: idx_audit_logs_created; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_audit_logs_created ON public.audit_logs USING btree (created_at);


--
-- Name: idx_audit_logs_event_type; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_audit_logs_event_type ON public.audit_logs USING btree (event_type);


--
-- Name: idx_audit_logs_user; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_audit_logs_user ON public.audit_logs USING btree (user_id);


--
-- Name: idx_blockchain_transactions_signature; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_blockchain_transactions_signature ON public.blockchain_transactions USING btree (signature);


--
-- Name: idx_blockchain_transactions_status; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_blockchain_transactions_status ON public.blockchain_transactions USING btree (status);


--
-- Name: idx_blockchain_transactions_submitted; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_blockchain_transactions_submitted ON public.blockchain_transactions USING btree (submitted_at);


--
-- Name: idx_blockchain_transactions_user; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_blockchain_transactions_user ON public.blockchain_transactions USING btree (user_id);


--
-- Name: idx_certificates_status; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_certificates_status ON public.erc_certificates USING btree (status);


--
-- Name: idx_certificates_type; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_certificates_type ON public.erc_certificates USING btree (certificate_type);


--
-- Name: idx_certificates_wallet; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_certificates_wallet ON public.erc_certificates USING btree (wallet_address);


--
-- Name: idx_erc_certificates_tx_signature; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_erc_certificates_tx_signature ON public.erc_certificates USING btree (blockchain_tx_signature);


--
-- Name: idx_erc_certificates_user; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_erc_certificates_user ON public.erc_certificates USING btree (user_id);


--
-- Name: idx_erc_transfers_certificate; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_erc_transfers_certificate ON public.erc_certificate_transfers USING btree (certificate_id);


--
-- Name: idx_erc_transfers_from_user; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_erc_transfers_from_user ON public.erc_certificate_transfers USING btree (from_user_id);


--
-- Name: idx_erc_transfers_to_user; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_erc_transfers_to_user ON public.erc_certificate_transfers USING btree (to_user_id);


--
-- Name: idx_market_epochs_number; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_market_epochs_number ON public.market_epochs USING btree (epoch_number);


--
-- Name: idx_market_epochs_status; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_market_epochs_status ON public.market_epochs USING btree (status);


--
-- Name: idx_market_epochs_time; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_market_epochs_time ON public.market_epochs USING btree (start_time, end_time);


--
-- Name: idx_meter_readings_meter; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_meter_readings_meter ON public.meter_readings USING btree (meter_id);


--
-- Name: idx_meter_readings_mint_tx; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_meter_readings_mint_tx ON public.meter_readings USING btree (mint_tx_signature);


--
-- Name: idx_meter_readings_reading_timestamp; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_meter_readings_reading_timestamp ON public.meter_readings USING btree (reading_timestamp);


--
-- Name: idx_meter_readings_timestamp; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_meter_readings_timestamp ON public.meter_readings USING btree ("timestamp");


--
-- Name: idx_meter_readings_user; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_meter_readings_user ON public.meter_readings USING btree (user_id);


--
-- Name: idx_meter_readings_wallet; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_meter_readings_wallet ON public.meter_readings USING btree (wallet_address);


--
-- Name: idx_order_matches_buy_order; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_order_matches_buy_order ON public.order_matches USING btree (buy_order_id);


--
-- Name: idx_order_matches_epoch; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_order_matches_epoch ON public.order_matches USING btree (epoch_id);


--
-- Name: idx_order_matches_sell_order; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_order_matches_sell_order ON public.order_matches USING btree (sell_order_id);


--
-- Name: idx_order_matches_status; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_order_matches_status ON public.order_matches USING btree (status);


--
-- Name: idx_settlements_buyer; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_settlements_buyer ON public.settlements USING btree (buyer_id);


--
-- Name: idx_settlements_epoch; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_settlements_epoch ON public.settlements USING btree (epoch_id);


--
-- Name: idx_settlements_seller; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_settlements_seller ON public.settlements USING btree (seller_id);


--
-- Name: idx_settlements_status; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_settlements_status ON public.settlements USING btree (status);


--
-- Name: idx_settlements_transaction; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_settlements_transaction ON public.settlements USING btree (transaction_hash);


--
-- Name: idx_trading_orders_created; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_trading_orders_created ON public.trading_orders USING btree (created_at);


--
-- Name: idx_trading_orders_epoch; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_trading_orders_epoch ON public.trading_orders USING btree (epoch_id);


--
-- Name: idx_trading_orders_type; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_trading_orders_type ON public.trading_orders USING btree (order_type);


--
-- Name: idx_trading_orders_user; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_trading_orders_user ON public.trading_orders USING btree (user_id);


--
-- Name: idx_user_activities_created; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_user_activities_created ON public.user_activities USING btree (created_at);


--
-- Name: idx_user_activities_type; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_user_activities_type ON public.user_activities USING btree (activity_type);


--
-- Name: idx_user_activities_user; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_user_activities_user ON public.user_activities USING btree (user_id);


--
-- Name: idx_users_email; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_users_email ON public.users USING btree (email);


--
-- Name: idx_users_email_verified; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_users_email_verified ON public.users USING btree (email_verified);


--
-- Name: idx_users_is_active; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_users_is_active ON public.users USING btree (is_active);


--
-- Name: idx_users_role; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_users_role ON public.users USING btree (role);


--
-- Name: idx_users_verification_expires; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_users_verification_expires ON public.users USING btree (email_verification_expires_at) WHERE (email_verification_expires_at IS NOT NULL);


--
-- Name: idx_users_verification_token; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_users_verification_token ON public.users USING btree (email_verification_token) WHERE (email_verification_token IS NOT NULL);


--
-- Name: idx_users_wallet; Type: INDEX; Schema: public; Owner: gridtokenx_user
--

CREATE INDEX idx_users_wallet ON public.users USING btree (wallet_address);


--
-- Name: blockchain_transactions update_blockchain_transactions_updated_at; Type: TRIGGER; Schema: public; Owner: gridtokenx_user
--

CREATE TRIGGER update_blockchain_transactions_updated_at BEFORE UPDATE ON public.blockchain_transactions FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();


--
-- Name: erc_certificates update_energy_certificates_updated_at; Type: TRIGGER; Schema: public; Owner: gridtokenx_user
--

CREATE TRIGGER update_energy_certificates_updated_at BEFORE UPDATE ON public.erc_certificates FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();


--
-- Name: market_epochs update_market_epochs_updated_at; Type: TRIGGER; Schema: public; Owner: gridtokenx_user
--

CREATE TRIGGER update_market_epochs_updated_at BEFORE UPDATE ON public.market_epochs FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();


--
-- Name: meter_readings update_meter_readings_updated_at; Type: TRIGGER; Schema: public; Owner: gridtokenx_user
--

CREATE TRIGGER update_meter_readings_updated_at BEFORE UPDATE ON public.meter_readings FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();


--
-- Name: order_matches update_order_matches_updated_at; Type: TRIGGER; Schema: public; Owner: gridtokenx_user
--

CREATE TRIGGER update_order_matches_updated_at BEFORE UPDATE ON public.order_matches FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();


--
-- Name: settlements update_settlements_updated_at; Type: TRIGGER; Schema: public; Owner: gridtokenx_user
--

CREATE TRIGGER update_settlements_updated_at BEFORE UPDATE ON public.settlements FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();


--
-- Name: trading_orders update_trading_orders_updated_at; Type: TRIGGER; Schema: public; Owner: gridtokenx_user
--

CREATE TRIGGER update_trading_orders_updated_at BEFORE UPDATE ON public.trading_orders FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();


--
-- Name: users update_users_updated_at; Type: TRIGGER; Schema: public; Owner: gridtokenx_user
--

CREATE TRIGGER update_users_updated_at BEFORE UPDATE ON public.users FOR EACH ROW EXECUTE FUNCTION public.update_updated_at_column();


--
-- Name: audit_logs audit_logs_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.audit_logs
    ADD CONSTRAINT audit_logs_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: blockchain_transactions blockchain_transactions_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.blockchain_transactions
    ADD CONSTRAINT blockchain_transactions_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: erc_certificate_transfers erc_certificate_transfers_certificate_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.erc_certificate_transfers
    ADD CONSTRAINT erc_certificate_transfers_certificate_id_fkey FOREIGN KEY (certificate_id) REFERENCES public.erc_certificates(id) ON DELETE CASCADE;


--
-- Name: erc_certificate_transfers erc_certificate_transfers_from_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.erc_certificate_transfers
    ADD CONSTRAINT erc_certificate_transfers_from_user_id_fkey FOREIGN KEY (from_user_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: erc_certificate_transfers erc_certificate_transfers_to_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.erc_certificate_transfers
    ADD CONSTRAINT erc_certificate_transfers_to_user_id_fkey FOREIGN KEY (to_user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: erc_certificates erc_certificates_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.erc_certificates
    ADD CONSTRAINT erc_certificates_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: order_matches fk_order_matches_settlement; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.order_matches
    ADD CONSTRAINT fk_order_matches_settlement FOREIGN KEY (settlement_id) REFERENCES public.settlements(id) ON DELETE SET NULL;


--
-- Name: meter_readings meter_readings_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.meter_readings
    ADD CONSTRAINT meter_readings_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: order_matches order_matches_buy_order_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.order_matches
    ADD CONSTRAINT order_matches_buy_order_id_fkey FOREIGN KEY (buy_order_id) REFERENCES public.trading_orders(id) ON DELETE CASCADE;


--
-- Name: order_matches order_matches_epoch_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.order_matches
    ADD CONSTRAINT order_matches_epoch_id_fkey FOREIGN KEY (epoch_id) REFERENCES public.market_epochs(id) ON DELETE CASCADE;


--
-- Name: order_matches order_matches_sell_order_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.order_matches
    ADD CONSTRAINT order_matches_sell_order_id_fkey FOREIGN KEY (sell_order_id) REFERENCES public.trading_orders(id) ON DELETE CASCADE;


--
-- Name: settlements settlements_buyer_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.settlements
    ADD CONSTRAINT settlements_buyer_id_fkey FOREIGN KEY (buyer_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: settlements settlements_epoch_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.settlements
    ADD CONSTRAINT settlements_epoch_id_fkey FOREIGN KEY (epoch_id) REFERENCES public.market_epochs(id) ON DELETE CASCADE;


--
-- Name: settlements settlements_seller_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.settlements
    ADD CONSTRAINT settlements_seller_id_fkey FOREIGN KEY (seller_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: trading_orders trading_orders_epoch_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.trading_orders
    ADD CONSTRAINT trading_orders_epoch_id_fkey FOREIGN KEY (epoch_id) REFERENCES public.market_epochs(id) ON DELETE SET NULL;


--
-- Name: trading_orders trading_orders_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.trading_orders
    ADD CONSTRAINT trading_orders_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: user_activities user_activities_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: gridtokenx_user
--

ALTER TABLE ONLY public.user_activities
    ADD CONSTRAINT user_activities_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- PostgreSQL database dump complete
--

\unrestrict ZvrIBqD36RU79rQo48bQIrYdxpm5PlF34Q38UukoAqi09pq1n4HmdKKlCrcRfQr

