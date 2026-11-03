# Compare

The seven boutique fitness players side by side: business model and API first, then 58 capabilities. Tags are explained in the [Market Scan method](../README.md#reading-the-feature-maps).

## Business model and platform

Four of the seven belong to three private-equity-backed owners; WellnessLiving, Arketa and Walla are independent. API access runs from gated review to fully self-serve.

|  | Mindbody | Mariana Tek | ABC Glofox | WellnessLiving | Momence | Arketa | Walla |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Owner | Playlist (merged with EGYM) | Xplor | ABC Fitness (Thoma Bravo) | Founder-led | Xplor | Venture-backed | Venture-backed |
| Entry price (USD/month) | $79 per location | Quote | From $99 | $69 | $0 plus 5% fee | Quote | $320 |
| Full pricing published | Starter only | No | Headline only | Yes | Yes | No | Starting prices |
| Payment processor | Own; others allowed | Stripe only | Stripe, GoCardless | Paragon or Nuvei only | Own, with reported surcharge | Stripe | Not confirmed |
| API access | Developer account; per-call fees reported | Partner review, 1 to 3 weeks | Email request, per branch | NDA and agreement | Self-serve OAuth clients | Self-serve key | Public API; gating not confirmed |
| OpenAPI spec | Third-party only | Not found | Yes | Yes | Yes | Not found | Not found |
| Webhooks | HMAC-signed | Yes | Signed CDC, at-least-once | Shared at onboarding | Experimental | Not found | Yes |
| Sandbox | Yes | Yes | Staging on request | Staging host | No | No | Not found |

## Feature matrix

58 capabilities harmonized from the seven feature maps (processor and API details are in the table above). About 20 appear at all seven players, which makes them v1 table stakes.

| Area | Capability | Our tag | Mindbody | Mariana Tek | ABC Glofox | WellnessLiving | Momence | Arketa | Walla |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Identity | Role-based staff permissions | Platform | Permission groups | Per location | Four roles plus matrix | Yes | Access levels | Yes | Yes |
| Identity | SSO or two-factor | Platform | Not found | SSO | Not found | Not found | Two-factor | Not found | Not found |
| Identity | Who owns the customer login | Platform | Platform-wide account | Brand | Not found | Not found | Platform-wide account | Not found | Not found |
| Org | Multi-location | Platform | Datashare | Brand, studio, HQ | Linked branches | Enterprise Cloud | Corporate dashboard | Yes | Yes |
| Org | Franchise royalties | Platform + config | Not found | Fee portal | Not found | Not found | Franchise dashboard | Not found | Not found |
| Customers | Profiles, notes, history | Platform | Yes | Yes | Yes | Yes | Yes | Yes | Yes |
| Customers | Family or child accounts | Platform | Not found | Not found | Yes | Not found | Yes, shared plans | Not found | Not found |
| Customers | Waivers and agreements | Platform | Yes | Yes | Boost tier | Yes | Yes | Yes | Yes |
| Customers | Custom forms and surveys | Platform | Not found | Forms, landing pages | Lead forms | Yes | Not found | Not found | Lead forms |
| Customers | Lead management | Platform | Ultimate tier | Xplor Growth | Boost or XLerate | Business tier | Yes | Yes | Yes |
| Scheduling | Recurring classes | Platform + config | Yes | Yes | Yes | Yes | Yes | Yes | Yes |
| Scheduling | Appointments | Platform + config | Yes | Yes | Yes | Yes | Yes | Yes | Yes |
| Scheduling | Courses or trainings | Platform | Yes | Not found | Yes | Not found | Video courses | Trainings | Not found |
| Scheduling | Events and workshops | Platform | Yes | Not found | Yes | Yes | Yes | Yes | Yes |
| Scheduling | Rooms, resources, facilities | Platform + config | Accelerate tier | Rooms | Facilities | Assets | Not found | Not found | Not found |
| Booking | Waitlists | Platform + config | Yes | Yes | Auto-book and charge | Yes | Yes | Yes | Yes |
| Booking | Spot selection | Vertical | Accelerate tier | Yes | Yes | Yes | Not found | Yes | Yes |
| Booking | Booking and cancellation rules | Platform + config | Per service | Yes | Yes | Not found | Per class | Yes | Not found |
| Booking | Guest or buddy passes | Vertical | Not found | Bring-a-guest | Guest bookings | Not found | Not found | Not found | Buddy passes |
| Booking | Late-cancel and no-show fees | Vertical | Yes | Yes | Reporting only | Not found | Not found | Yes | Yes |
| Attendance | Self check-in | Platform + config | Yes | Including geo | Yes | Yes | Yes | Check-in screen | Yes |
| Attendance | Door access | Integration | Partners | Not found | Access control | Brivo | Kisi | Not found | Not found |
| Commerce | Recurring memberships | Platform | Yes | Yes | Yes | Yes | Yes | Yes | Yes |
| Commerce | Credit packs | Platform | Yes | Yes | Yes | Yes | Yes | Yes | Yes |
| Commerce | Freeze or transfer | Platform | Not found | Freeze | Not found | Not found | Both | Not found | Not found |
| Commerce | Intro or trial offers | Vertical | Yes | Yes | Trial memberships | Not found | Yes | Yes | Not found |
| Commerce | Discounts and promo codes | Platform | Accelerate tier | Yes | Yes | Yes | Yes | Yes | Yes |
| Commerce | Dynamic or sliding pricing | Platform + config | Dynamic | Not found | Not found | Not found | Sliding scale | Not found | Not found |
| Commerce | Bundles | Platform | Not found | Yes | Not found | Not found | Not found | Yes | Not found |
| Commerce | Invoices or pay later | Platform | Not found | Not found | Not found | Not found | Pay later | Invoices | Not found |
| Commerce | POS, retail, inventory | Platform | Yes | Yes | Boost tier | Yes | Yes | Yes | Yes |
| Commerce | Gift cards | Platform | Not found | Yes | Not found | Not found | Yes | Not found | Not found |
| Commerce | Add-ons with a reservation | Vertical | Not found | Yes | Not found | Not found | Not found | Not found | Not found |
| Commerce | Tax or VAT handling | Platform | Not found | Not found | VAT | Not found | Not found | Not found | Not found |
| Payments | Failed-payment handling | Platform + config | Not found | Not found | NSF fee | Collections partner | Not found | Not found | Not found |
| Messaging | Transactional email and SMS | Platform | Yes | Yes | Yes | Yes | Yes | Yes | Yes |
| Messaging | Two-way SMS inbox | Platform | Not found | Not found | Not found | Message Center | Yes | Texting and calls | Pro plan |
| Marketing | Automations and segments | Platform | Ultimate or add-on | Separate system | XLerate add-on | Business tier | Yes | Yes | Pro plan |
| Marketing | Referral programs | Vertical | Yes | Yes | Not found | Not found | Yes | Not found | Via partner |
| Engagement | Loyalty, rewards, milestones | Vertical | Not found | Yes | Not found | Rewards | SuperFans | Rewards, milestones | Milestones |
| Engagement | Reviews | Platform + config | In Mindbody app | NPS | Not found | Yes | Not found | Yes | Yes |
| Staff | Availability and substitutes | Vertical | Subs by text | Not found | Trainer availability | Not found | Not found | Sub management | Not found |
| Staff | Payroll calculation | Platform | Yes | Yes | Yes | Yes | Reporting | Richest pay rules | Reports |
| Staff | Payroll processing and tax | Integration | ADP, Paychex export | Direct deposit | Not found | Not found | Not found | Direct deposit, tax filing | Export only |
| Insights | Reporting | Platform + config | Yes | Yes | Yes | Yes | Yes | Yes | Yes |
| Insights | Churn prediction | Platform + config | AI Insights | Not found | Churn predictor | Isaac | Not found | Yes | WallaPredict |
| Insights | Peer benchmarks | Platform + config | About 40K businesses | Not found | Not found | Not found | Not found | Yes | Not found |
| AI | AI front-desk agent | Integration | Ultimate or add-on | Not found | ABC AI Agents | CAASI | $399 add-on | Not found | Not found |
| Content | On-demand video | (new) | Partners | Partners | Live and on-demand | Go Virtual | Native, with courses | Native | Native |
| Platform | Branded client app | Platform + config | Add-on | Included | Add-on, or Elite | BusinessPro tier | Momence-branded | Yes | Yes |
| Platform | Staff app | Platform + config | Yes | Biz App | Pro app | Elevate | Yes | Business app | Not found |
| Platform | Website builder | Platform + config | Included | Not found | Embeds | Presence, extra fee | Checkout pages | Embeds | Walla Websites |
| Platform | Localization | Platform | 130+ countries | Not found | 17 languages | Not found | Multi-language | Not found | Not found |
| Distribution | Consumer marketplace | (new) | 3M+ user app | No | No | Directory | Multi-studio app | No | No |
| Distribution | ClassPass and aggregators | Integration | Owns ClassPass | Yes | Yes | Yes | Yes | Yes | Yes |
| Distribution | Reserve with Google | Integration | Yes | Not found | Not found | Yes | Not found | Not found | Not found |
| Finance | Accounting | (new) | Not found | Partners | Not found | QuickBooks | QuickBooks, Xero | Built in | Not found |
| Finance | Business financing | (new) | Mindbody Capital | Xplor Capital | Not found | Not found | Not found | Not found | Not found |
