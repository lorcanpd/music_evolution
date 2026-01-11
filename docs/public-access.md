# Exposing Music Evolution to the Public Internet

This guide explains two methods for making your Music Evolution instance accessible from the internet.

## Overview

| Method | Difficulty | Requirements | Pros | Cons |
|--------|------------|--------------|------|------|
| **Port Forwarding** | Moderate | Router access, Static IP (or DDNS) | Full control, no third-party | Requires router config, exposes your IP |
| **Cloudflare Tunnel** | Easy | Cloudflare account (free) | No router config, hides IP, built-in protection | Depends on Cloudflare |

**Recommendation for beginners: Cloudflare Tunnel** - It's simpler to set up and provides additional security benefits.

---

## Method A: Router Port Forwarding + Let's Encrypt

This method forwards traffic directly from your router to martin.

### Prerequisites

- Access to your router's admin panel
- A domain name (or use a free DDNS service)
- Ports 80 and 443 available on your network

### Step 1: Get a Domain Name

**Option A: Buy a domain** (~$10-15/year)
- Namecheap, Cloudflare, Google Domains, etc.
- Point the A record to your public IP

**Option B: Use a free DDNS service**
- [Duck DNS](https://www.duckdns.org/) - Free subdomains like `yourname.duckdns.org`
- [No-IP](https://www.noip.com/) - Free dynamic DNS

### Step 2: Configure Your Router

1. Log into your router (usually `192.168.1.1` or `192.168.0.1`)
2. Find "Port Forwarding" or "NAT" settings
3. Add two forwarding rules:

   | External Port | Internal IP | Internal Port | Protocol |
   |---------------|-------------|---------------|----------|
   | 80 | martin's IP | 80 | TCP |
   | 443 | martin's IP | 443 | TCP |

4. Save and apply

To find martin's IP:
```bash
# On martin
ip addr show | grep "inet "
```

### Step 3: Install Certbot and Get Certificates

On martin:

```bash
# Install certbot
sudo apt update
sudo apt install certbot

# Stop nginx temporarily
docker compose -f docker-compose.yml -f compose.prod.yml stop proxy

# Get certificates (standalone mode)
sudo certbot certonly --standalone -d YOUR_DOMAIN.example.com

# Copy certificates to the service directory
sudo cp /etc/letsencrypt/live/YOUR_DOMAIN.example.com/fullchain.pem /srv/services/music-evo/certs/
sudo cp /etc/letsencrypt/live/YOUR_DOMAIN.example.com/privkey.pem /srv/services/music-evo/certs/
sudo chown $USER:$USER /srv/services/music-evo/certs/*.pem
```

### Step 4: Configure Nginx for TLS

```bash
# Copy the TLS config
cp /srv/services/music-evo/deploy/nginx/conf.d/default-tls.conf.example \
   /srv/services/music-evo/nginx/conf.d/default.conf

# Edit and replace YOUR_DOMAIN.example.com
nano /srv/services/music-evo/nginx/conf.d/default.conf
```

### Step 5: Start the Services

```bash
docker compose -f docker-compose.yml -f compose.prod.yml up -d
```

### Step 6: Set Up Certificate Auto-Renewal

```bash
# Create renewal script
sudo nano /etc/cron.weekly/renew-music-evo-cert
```

```bash
#!/bin/bash
cd /srv/services/music-evo
docker compose -f docker-compose.yml -f compose.prod.yml stop proxy
certbot renew --quiet
cp /etc/letsencrypt/live/YOUR_DOMAIN.example.com/fullchain.pem /srv/services/music-evo/certs/
cp /etc/letsencrypt/live/YOUR_DOMAIN.example.com/privkey.pem /srv/services/music-evo/certs/
docker compose -f docker-compose.yml -f compose.prod.yml start proxy
```

```bash
sudo chmod +x /etc/cron.weekly/renew-music-evo-cert
```

### Troubleshooting Port Forwarding

- **Can't access from outside**: Check your ISP doesn't block ports 80/443 (some do)
- **Works internally but not externally**: NAT loopback might not be supported; test from a mobile device on cellular
- **Certificates fail**: Ensure port 80 is reachable for the ACME challenge

---

## Method B: Cloudflare Tunnel (Recommended for Beginners)

Cloudflare Tunnel creates a secure outbound connection from martin to Cloudflare's network. No port forwarding required.

### Prerequisites

- A domain name (you can buy one through Cloudflare or transfer an existing one)
- Free Cloudflare account

### Step 1: Set Up Cloudflare

1. Create account at https://dash.cloudflare.com
2. Add your domain to Cloudflare
3. Update your domain's nameservers to Cloudflare's (instructions provided)
4. Wait for DNS propagation (up to 24 hours, usually faster)

### Step 2: Install cloudflared on Martin

```bash
# Download and install cloudflared
curl -L --output cloudflared.deb https://github.com/cloudflare/cloudflared/releases/latest/download/cloudflared-linux-arm64.deb
sudo dpkg -i cloudflared.deb

# Authenticate with Cloudflare
cloudflared tunnel login
# This opens a browser - log in and authorize
```

### Step 3: Create a Tunnel

```bash
# Create the tunnel
cloudflared tunnel create music-evo

# Note the tunnel ID (e.g., a1b2c3d4-e5f6-7890-abcd-ef1234567890)
```

### Step 4: Configure the Tunnel

Create config file:
```bash
mkdir -p ~/.cloudflared
nano ~/.cloudflared/config.yml
```

```yaml
tunnel: YOUR_TUNNEL_ID
credentials-file: /home/YOUR_USER/.cloudflared/YOUR_TUNNEL_ID.json

ingress:
  - hostname: music-evo.yourdomain.com
    service: http://localhost:80
  - service: http_status:404
```

### Step 5: Route DNS Through the Tunnel

```bash
cloudflared tunnel route dns music-evo music-evo.yourdomain.com
```

### Step 6: Run the Tunnel as a Service

```bash
# Install as systemd service
sudo cloudflared --config /home/YOUR_USER/.cloudflared/config.yml service install

# Start the service
sudo systemctl start cloudflared
sudo systemctl enable cloudflared

# Check status
sudo systemctl status cloudflared
```

### Step 7: Update Nginx (No TLS Needed Locally)

With Cloudflare Tunnel, TLS is handled by Cloudflare. Your local nginx can use HTTP:

```bash
# Use the default HTTP config
cp /srv/services/music-evo/deploy/nginx/conf.d/default.conf \
   /srv/services/music-evo/nginx/conf.d/default.conf
```

Cloudflare provides:
- Automatic HTTPS to visitors
- DDoS protection
- Rate limiting (configurable in dashboard)
- Analytics

### Step 8: Restart Services

```bash
docker compose -f docker-compose.yml -f compose.prod.yml up -d
```

### Step 9: Enable Additional Cloudflare Security (Optional)

In Cloudflare Dashboard → Security:

1. **WAF Rules**: Block common attack patterns
2. **Rate Limiting**: Add rules for your endpoints
3. **Bot Fight Mode**: Enable to reduce bot traffic
4. **Browser Integrity Check**: Enable

### Troubleshooting Cloudflare Tunnel

```bash
# Check tunnel status
cloudflared tunnel info music-evo

# View tunnel logs
sudo journalctl -u cloudflared -f

# Test local connectivity
curl http://localhost:80
```

---

## Security Recommendations

Regardless of which method you choose:

1. **Keep software updated**
   ```bash
   sudo apt update && sudo apt upgrade
   ```

2. **Enable fail2ban** (optional, for port forwarding)
   ```bash
   sudo apt install fail2ban
   ```

3. **Monitor access logs**
   ```bash
   docker compose -f docker-compose.yml -f compose.prod.yml logs -f proxy
   ```

4. **Set up monitoring** (optional)
   - Uptime Robot (free) - https://uptimerobot.com
   - Cloudflare Analytics (if using Tunnel)

5. **Regular backups**
   ```bash
   # Backup database weekly
   docker compose -f docker-compose.yml -f compose.prod.yml exec postgres \
       pg_dump -U musicevo musicevo > /srv/services/music-evo/backups/backup_$(date +%Y%m%d).sql
   ```

## Comparison Summary

| Feature | Port Forwarding | Cloudflare Tunnel |
|---------|-----------------|-------------------|
| Setup complexity | Moderate | Easy |
| Router changes needed | Yes | No |
| Exposes home IP | Yes | No |
| DDoS protection | Basic (nginx) | Built-in |
| TLS certificate | Manual (Let's Encrypt) | Automatic |
| Cost | Free (with free DDNS) | Free |
| Latency | Direct | Slight overhead |
| Dependence on third-party | Minimal | Cloudflare |
