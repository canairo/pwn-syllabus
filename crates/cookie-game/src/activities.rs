//! Bakery activities are ordinary guest allocations, including their goals and deadlines.
use super::*;

pub const ACTIVITIES_ROOT: Address = Architecture::GLOBALS + 8;
const ACTIVITIES_SIZE: u64 = 128;
const ORDER_SIZE: u64 = 64;
const ORDER_COUNT: usize = 3;
pub mod activity_field {
    pub const RECIPE: u64 = 0;
    pub const CLICK_MULTIPLIER: u64 = 8;
    pub const PRODUCTION_MULTIPLIER: u64 = 16;
    pub const RECIPE_FLAGS: u64 = 24;
    pub const ORDERS_DONE: u64 = 48;
    pub const RUSH_WINS: u64 = 56;
    pub const RUSH_STATE: u64 = 64;
    pub const RUSH_TARGET: u64 = 72;
    pub const RUSH_PROGRESS: u64 = 80;
    pub const RUSH_REWARD: u64 = 88;
    pub const RUSH_DEADLINE: u64 = 96;
    pub const RUSH_NEXT: u64 = 104;
    pub const LAST_SHOWER: u64 = 112;
    pub const ORDERS: u64 = 120;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn offer(m: &mut Machine) {
        m.offer_rush().unwrap();
        let a = m.activities_address().unwrap();
        m.store(a + activity_field::RUSH_TARGET, 12, "test")
            .unwrap();
        m.store(a + activity_field::RUSH_REWARD, 50, "test")
            .unwrap();
    }
    #[test]
    fn orders_progress_together_and_claims_generate_new_goals() {
        let mut m = Machine::new(1234).unwrap();
        assert!(m.claim_order(0).is_err());
        for _ in 0..10 {
            m.click().unwrap();
        }
        m.claim_order(1).unwrap();
        assert_eq!(m.view().unwrap().cookies, 35);
        assert!(
            m.claim_order(1).is_err(),
            "the same reward cannot be collected twice"
        );
        m.claim_order(0).unwrap();
        assert_eq!(
            m.activities_view().unwrap().orders[1].progress,
            0,
            "reward cookies are not manual clicks"
        );
        for _ in 0..40 {
            m.click().unwrap();
        }
        for _ in 0..5 {
            m.buy_generator(0).unwrap();
        }
        m.claim_order(2).unwrap();
        let v = m.view().unwrap();
        assert_eq!(v.cookies, 57);
        assert_eq!(v.activities.orders_done, 3);
        assert!(v.activities.orders[2].target > v.activities.orders[2].progress);
        assert!(v.activities.orders[2].target - v.activities.orders[2].progress >= 3);
        assert!(
            v.achievements[1],
            "order rewards contribute to lifetime cookies"
        );
    }
    #[test]
    fn generator_orders_follow_production_and_require_a_batch() {
        let mut m = Machine::new(1).unwrap();
        m.set(field::COOKIES, 7391, "test").unwrap();
        m.set(field::CPS, 216, "test").unwrap();
        let base = m.object(field::GENERATORS).unwrap();
        m.store(base, 12, "test").unwrap();
        m.refresh_order(2).unwrap();
        let order = m.order_view(2).unwrap();
        assert_eq!(
            order.generator, 3,
            "a bakery producing 216/s should work toward factories"
        );
        assert_eq!(order.target, 3);
        assert_eq!(order.reward, (12_000 + 13_800 + 15_870) / 2);
        assert!(m.claim_order(2).is_err());
    }
    #[test]
    fn recipes_charge_once_and_change_real_clicks_and_passive_production() {
        let mut m = Machine::new(1).unwrap();
        assert!(m.select_recipe(1).is_err());
        m.set(field::COOKIES, 1000, "test").unwrap();
        m.buy_generator(0).unwrap();
        m.select_recipe(1).unwrap();
        m.click().unwrap();
        assert_eq!(m.view().unwrap().cookies, 948);
        assert_eq!(m.view().unwrap().click_power, 3);
        assert_eq!(m.view().unwrap().cps, 1);
        m.select_recipe(2).unwrap();
        m.click().unwrap();
        for _ in 0..TICKS_PER_SECOND {
            m.advance_tick().unwrap();
        }
        assert_eq!(m.view().unwrap().cookies, 872);
        assert_eq!(m.view().unwrap().generators[0].per_unit, 3);
        let before = m.value(field::COOKIES).unwrap();
        m.select_recipe(1).unwrap();
        m.select_recipe(2).unwrap();
        assert_eq!(m.value(field::COOKIES).unwrap(), before);
        assert!(m.select_recipe(usize::MAX).is_err());
    }
    #[test]
    fn rush_rewards_survive_deadline_and_cannot_be_claimed_twice() {
        let mut m = Machine::new(1).unwrap();
        offer(&mut m);
        m.start_rush().unwrap();
        assert!(m.start_rush().is_err());
        for _ in 0..12 {
            m.click().unwrap();
        }
        for _ in 0..400 {
            m.advance_tick().unwrap();
        }
        assert_eq!(m.activities_view().unwrap().rush_state, RushState::Ready);
        m.claim_rush().unwrap();
        assert_eq!(m.view().unwrap().cookies, 62);
        assert_eq!(m.activities_view().unwrap().rush_wins, 1);
        assert!(m.claim_rush().is_err());
        assert!(m.start_rush().is_err());
        let next = m.activity_value(activity_field::RUSH_NEXT).unwrap();
        assert!((90 * TICKS_PER_SECOND..=180 * TICKS_PER_SECOND).contains(&(next - m.tick)));
        m.tick = next - 1;
        m.advance_tick().unwrap();
        assert_eq!(m.activities_view().unwrap().rush_state, RushState::Offered);
        m.start_rush().unwrap();
        assert!((12..=20).contains(&m.activities_view().unwrap().rush_target));
    }
    #[test]
    fn rush_timeout_is_exact_and_failure_has_no_penalty() {
        let mut m = Machine::new(1).unwrap();
        offer(&mut m);
        m.start_rush().unwrap();
        for _ in 0..399 {
            m.advance_tick().unwrap();
        }
        m.click().unwrap();
        assert_eq!(m.activities_view().unwrap().rush_progress, 1);
        m.advance_tick().unwrap();
        m.click().unwrap();
        assert_eq!(m.activities_view().unwrap().rush_state, RushState::Missed);
        assert_eq!(m.activities_view().unwrap().rush_progress, 1);
        assert_eq!(m.view().unwrap().cookies, 2);
        assert!(m.claim_rush().is_err());
        offer(&mut m);
        m.start_rush().unwrap();
        assert_eq!(m.activities_view().unwrap().rush_progress, 0);
    }
    #[test]
    fn golden_batch_offers_exclusive_instant_cookies_or_a_boost() {
        let mut m = Machine::new(1).unwrap();
        assert!(m.claim_cookie_shower().is_err());
        let next = m
            .memory
            .read_u64(m.object(field::EFFECTS).unwrap() + 16)
            .unwrap();
        assert!((180 * TICKS_PER_SECOND..=300 * TICKS_PER_SECOND).contains(&next));
        m.tick = next;
        let mut boosted = m.clone();
        boosted.claim_gift().unwrap();
        boosted.click().unwrap();
        assert_eq!(boosted.view().unwrap().cookies, 7);
        let preview = m.activities_view().unwrap().shower_reward;
        m.claim_cookie_shower().unwrap();
        assert_eq!(m.view().unwrap().cookies, preview);
        assert_eq!(preview, 100);
        assert_eq!(m.view().unwrap().boost_remaining, 0);
        assert!(!m.view().unwrap().gift_available);
        assert!(m.claim_gift().is_err());
        assert!(m.claim_cookie_shower().is_err());
        let next = m
            .memory
            .read_u64(m.object(field::EFFECTS).unwrap() + 16)
            .unwrap();
        assert!((240 * TICKS_PER_SECOND..=420 * TICKS_PER_SECOND).contains(&(next - m.tick)));
    }
    #[test]
    fn guest_edits_control_recipe_order_rewards_and_rush_deadlines() {
        let mut m = Machine::new(1).unwrap();
        let a = m.activities_address().unwrap();
        m.write_memory(a + activity_field::CLICK_MULTIPLIER, &10u64.to_le_bytes())
            .unwrap();
        m.click().unwrap();
        assert_eq!(m.view().unwrap().cookies, 10);
        let orders = m.orders_address().unwrap();
        m.write_memory(orders + order_field::TARGET, &10u64.to_le_bytes())
            .unwrap();
        m.write_memory(orders + order_field::REWARD, &1234u64.to_le_bytes())
            .unwrap();
        m.claim_order(0).unwrap();
        assert_eq!(m.view().unwrap().cookies, 1244);
        offer(&mut m);
        m.start_rush().unwrap();
        m.write_memory(a + activity_field::RUSH_DEADLINE, &1u64.to_le_bytes())
            .unwrap();
        m.advance_tick().unwrap();
        assert_eq!(m.activities_view().unwrap().rush_state, RushState::Missed);
        m.write_memory(ACTIVITIES_ROOT, &u64::MAX.to_le_bytes())
            .unwrap();
        assert!(m.view().is_err());
        assert!(m.start_rush().is_err());
        assert!(m.advance_tick().is_err());
    }
    #[test]
    fn prestige_rebases_orders_and_cancels_rush_without_erasing_recipe_unlocks() {
        let mut m = Machine::new(1).unwrap();
        m.set(field::COOKIES, 1_000_000, "test").unwrap();
        m.set(field::LIFETIME, 1_000_000, "test").unwrap();
        m.select_recipe(1).unwrap();
        m.buy_generator(0).unwrap();
        offer(&mut m);
        m.start_rush().unwrap();
        m.prestige().unwrap();
        let v = m.view().unwrap();
        assert_eq!(v.activities.rush_state, RushState::Idle);
        assert_eq!(v.activities.orders[0].progress, 0);
        assert_eq!(v.activities.orders[2].progress, 0);
        assert!(v.activities.recipes_unlocked[1]);
        assert_eq!(v.click_power, 6);
    }
}
mod order_field {
    pub const KIND: u64 = 0;
    pub const TARGET: u64 = 8;
    pub const BASELINE: u64 = 16;
    pub const REWARD: u64 = 24;
    pub const GENERATOR: u64 = 32;
    pub const CUSTOMER: u64 = 40;
    pub const COMPLETIONS: u64 = 48;
}
#[derive(Debug, Clone, Copy)]
pub struct RecipeSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub price: u64,
    pub click_multiplier: u64,
    pub production_multiplier: u64,
}
pub const RECIPES: [RecipeSpec; 3] = [
    RecipeSpec {
        name: "Classic",
        description: "The original recipe",
        price: 0,
        click_multiplier: 1,
        production_multiplier: 1,
    },
    RecipeSpec {
        name: "Chocolate chip",
        description: "3× cookies per click",
        price: 40,
        click_multiplier: 3,
        production_multiplier: 1,
    },
    RecipeSpec {
        name: "Vanilla crunch",
        description: "3× generator production",
        price: 80,
        click_multiplier: 1,
        production_multiplier: 3,
    },
];
const CUSTOMERS: [&str; 6] = [
    "Picnic club",
    "School fair",
    "Space camp",
    "Birthday party",
    "Dinosaur museum",
    "Robot workshop",
];
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderKind {
    Bake,
    Click,
    Generator,
}
#[derive(Debug, Clone)]
pub struct OrderView {
    pub kind: OrderKind,
    pub customer: &'static str,
    pub target: u64,
    pub progress: u64,
    pub reward: u64,
    pub generator: usize,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RushState {
    Idle,
    Running,
    Ready,
    Missed,
    Offered,
}
#[derive(Debug, Clone)]
pub struct ActivitiesView {
    pub recipe: usize,
    pub recipes_unlocked: [bool; 3],
    pub orders: Vec<OrderView>,
    pub orders_done: u64,
    pub rush_wins: u64,
    pub rush_state: RushState,
    pub rush_target: u64,
    pub rush_progress: u64,
    pub rush_reward: u64,
    pub rush_remaining: u64,
    pub rush_cooldown: u64,
    pub shower_reward: u64,
}
impl Machine {
    /// Used for fresh games and the explicit upgrade of saves predating bakery activities.
    /// The original player layout, addresses, RNG and clock remain unchanged.
    pub fn initialize_activities(&mut self) -> Result<()> {
        if self.memory.read_ptr64(ACTIVITIES_ROOT)? != 0 {
            return Ok(());
        }
        let a = self.heap.allocate(ACTIVITIES_SIZE, 8, "BakeryActivities")?;
        let orders = self
            .heap
            .allocate(ORDER_SIZE * ORDER_COUNT as u64, 8, "BakeryOrder[]")?;
        self.write(
            a,
            &[0; ACTIVITIES_SIZE as usize],
            WriteSource::Initialization,
            "initialize_activities",
            Some(a),
        )?;
        self.write(
            orders,
            &[0; (ORDER_SIZE * ORDER_COUNT as u64) as usize],
            WriteSource::Initialization,
            "initialize_orders",
            Some(orders),
        )?;
        for (offset, value) in [
            (activity_field::CLICK_MULTIPLIER, 1),
            (activity_field::PRODUCTION_MULTIPLIER, 1),
            (activity_field::RECIPE_FLAGS, 1),
            (activity_field::RUSH_TARGET, 12),
            (activity_field::RUSH_REWARD, 50),
            (activity_field::ORDERS, orders),
        ] {
            self.store_init(a + offset, value)?;
        }
        // A legacy snapshot may already contain deliberately corrupted guest pointers.
        // Keep those bytes; new order baselines need not dereference a broken object.
        let clicks = self
            .object(field::STATS)
            .and_then(|s| Ok(self.memory.read_u64(s)?))
            .unwrap_or(0);
        for (i, (target, baseline, reward)) in [
            (20, self.value(field::LIFETIME).unwrap_or(0), 30),
            (10, clicks, 25),
            (5, 0, 48),
        ]
        .into_iter()
        .enumerate()
        {
            let o = orders + i as u64 * ORDER_SIZE;
            for (offset, value) in [
                (order_field::KIND, i as u64),
                (order_field::TARGET, target),
                (order_field::BASELINE, baseline),
                (order_field::REWARD, reward),
                (order_field::CUSTOMER, i as u64),
            ] {
                self.store_init(o + offset, value)?;
            }
        }
        self.store_init(ACTIVITIES_ROOT, a)
    }
    pub fn activities_address(&self) -> Result<Address> {
        let a = self.memory.read_ptr64(ACTIVITIES_ROOT)?;
        self.memory.read(a, ACTIVITIES_SIZE)?;
        Ok(a)
    }
    fn orders_address(&self) -> Result<Address> {
        let a = self
            .memory
            .read_ptr64(self.activities_address()? + activity_field::ORDERS)?;
        self.memory.read(a, ORDER_SIZE * ORDER_COUNT as u64)?;
        Ok(a)
    }
    fn activity_value(&self, offset: u64) -> Result<u64> {
        Ok(self.memory.read_u64(self.activities_address()? + offset)?)
    }
    pub(crate) fn recipe_click_multiplier(&self) -> Result<u64> {
        self.activity_value(activity_field::CLICK_MULTIPLIER)
    }
    pub(crate) fn recipe_production_multiplier(&self) -> Result<u64> {
        self.activity_value(activity_field::PRODUCTION_MULTIPLIER)
    }
    pub fn select_recipe(&mut self, index: usize) -> Result<()> {
        let spec = RECIPES.get(index).ok_or(GameError::Locked)?;
        let a = self.activities_address()?;
        let flag = a + activity_field::RECIPE_FLAGS + index as u64 * 8;
        if self.memory.read_u64(flag)? == 0 {
            self.spend(spec.price)?;
            self.store(flag, 1, "unlock_recipe")?;
        }
        for (offset, value) in [
            (activity_field::RECIPE, index as u64),
            (activity_field::CLICK_MULTIPLIER, spec.click_multiplier),
            (
                activity_field::PRODUCTION_MULTIPLIER,
                spec.production_multiplier,
            ),
        ] {
            self.store(a + offset, value, "choose_recipe")?;
        }
        Ok(())
    }
    fn order_view(&self, index: usize) -> Result<OrderView> {
        if index >= ORDER_COUNT {
            return Err(GameError::Locked);
        }
        let a = self.orders_address()? + index as u64 * ORDER_SIZE;
        let value = |offset| self.memory.read_u64(a + offset);
        let kind = match value(order_field::KIND)? {
            0 => OrderKind::Bake,
            1 => OrderKind::Click,
            2 => OrderKind::Generator,
            _ => return Err(GameError::InvalidState("unknown order type".into())),
        };
        let generator = value(order_field::GENERATOR)?;
        if generator >= GENERATORS.len() as u64 {
            return Err(GameError::InvalidState("unknown order generator".into()));
        }
        let progress = match kind {
            OrderKind::Bake => self
                .value(field::LIFETIME)?
                .saturating_sub(value(order_field::BASELINE)?),
            OrderKind::Click => self
                .memory
                .read_u64(self.object(field::STATS)?)?
                .saturating_sub(value(order_field::BASELINE)?),
            OrderKind::Generator => self
                .memory
                .read_u64(self.object(field::GENERATORS)? + generator * GENERATOR_SIZE)?,
        };
        Ok(OrderView {
            kind,
            customer: CUSTOMERS[(value(order_field::CUSTOMER)? % CUSTOMERS.len() as u64) as usize],
            target: value(order_field::TARGET)?,
            progress,
            reward: value(order_field::REWARD)?,
            generator: generator as usize,
        })
    }
    pub fn claim_order(&mut self, index: usize) -> Result<()> {
        let order = self.order_view(index)?;
        if order.progress < order.target {
            return Err(GameError::Locked);
        }
        self.award(order.reward, "complete_order")?;
        let a = self.activities_address()?;
        self.add_at(a + activity_field::ORDERS_DONE, 1, "count_order")?;
        self.add_at(
            self.orders_address()? + index as u64 * ORDER_SIZE + order_field::COMPLETIONS,
            1,
            "count_order",
        )?;
        self.refresh_order(index)
    }
    fn refresh_order(&mut self, index: usize) -> Result<()> {
        let o = self.orders_address()? + index as u64 * ORDER_SIZE;
        let random = self.next_random();
        let click = self.click_power()?;
        let cps = self
            .value(field::CPS)?
            .saturating_mul(self.recipe_production_multiplier()?);
        let (target, baseline, reward, generator) = match index {
            0 => {
                let goal = click
                    .saturating_mul(15 + random % 16)
                    .max(cps.saturating_mul(30))
                    .max(20);
                (
                    goal,
                    self.value(field::LIFETIME)?,
                    (goal / 2).saturating_add(20),
                    0,
                )
            }
            1 => {
                let goal = 12 + random % 14;
                (
                    goal,
                    self.memory.read_u64(self.object(field::STATS)?)?,
                    click
                        .saturating_mul(goal)
                        .max(cps.saturating_mul(20))
                        .max(25),
                    0,
                )
            }
            2 => self.generator_order_goal()?,
            _ => return Err(GameError::Locked),
        };
        for (offset, value) in [
            (order_field::TARGET, target),
            (order_field::BASELINE, baseline),
            (order_field::REWARD, reward),
            (order_field::GENERATOR, generator),
            (order_field::CUSTOMER, random % CUSTOMERS.len() as u64),
        ] {
            self.store(o + offset, value, "new_order")?;
        }
        Ok(())
    }
    fn generator_order_goal(&self) -> Result<(u64, u64, u64, u64)> {
        let budget = (self.value(field::COOKIES)? / 2)
            .max(self.click_power()?.saturating_mul(40))
            .max(
                self.value(field::CPS)?
                    .saturating_mul(self.recipe_production_multiplier()?)
                    .saturating_mul(60),
            );
        let tier = GENERATORS
            .iter()
            .rposition(|g| g.price <= budget)
            .unwrap_or(0) as u64;
        let g = self.object(field::GENERATORS)? + tier * GENERATOR_SIZE;
        let owned = self.memory.read_u64(g)?;
        let quantity = if owned == 0 && tier > 0 {
            3
        } else {
            owned.div_ceil(2).clamp(5, 25)
        };
        let mut price = self.memory.read_u64(g + 8)?;
        let mut cost = 0u64;
        for _ in 0..quantity {
            cost = cost.saturating_add(price);
            price = next_generator_price(price);
        }
        Ok((owned.saturating_add(quantity), 0, cost / 2, tier))
    }
    pub(crate) fn rebalance_short_generator_order(&mut self) -> Result<()> {
        let Ok(order) = self.order_view(2) else {
            return Ok(());
        };
        if order.kind != OrderKind::Generator
            || order.progress >= order.target
            || order.target - order.progress >= 3
        {
            return Ok(());
        }
        let (target, _, reward, generator) = self.generator_order_goal()?;
        let a = self.orders_address()? + 2 * ORDER_SIZE;
        for (offset, value) in [
            (order_field::TARGET, target),
            (order_field::REWARD, reward),
            (order_field::GENERATOR, generator),
        ] {
            self.store_init(a + offset, value)?;
        }
        Ok(())
    }
    pub fn start_rush(&mut self) -> Result<()> {
        let a = self.activities_address()?;
        let state = self.activity_value(activity_field::RUSH_STATE)?;
        if state != 4 || self.tick >= self.activity_value(activity_field::RUSH_DEADLINE)? {
            return Err(GameError::Locked);
        }
        self.store(a + activity_field::RUSH_STATE, 1, "start_baking_rush")?;
        self.store(a + activity_field::RUSH_PROGRESS, 0, "start_baking_rush")?;
        self.store(
            a + activity_field::RUSH_DEADLINE,
            self.tick.saturating_add(20 * TICKS_PER_SECOND),
            "start_baking_rush",
        )
    }
    fn offer_rush(&mut self) -> Result<()> {
        let a = self.activities_address()?;
        let target = 12 + self.next_random() % 9;
        let reward = self
            .click_power()?
            .saturating_mul(target)
            .saturating_mul(3)
            .max(
                self.value(field::CPS)?
                    .saturating_mul(self.recipe_production_multiplier()?)
                    .saturating_mul(15),
            )
            .max(50);
        for (offset, value) in [
            (activity_field::RUSH_STATE, 4),
            (activity_field::RUSH_TARGET, target),
            (activity_field::RUSH_PROGRESS, 0),
            (activity_field::RUSH_REWARD, reward),
            (
                activity_field::RUSH_DEADLINE,
                self.tick.saturating_add(30 * TICKS_PER_SECOND),
            ),
        ] {
            self.store(a + offset, value, "offer_baking_rush")?;
        }
        Ok(())
    }
    pub(crate) fn record_rush_click(&mut self) -> Result<()> {
        let a = self.activities_address()?;
        if self.activity_value(activity_field::RUSH_STATE)? == 1
            && self.tick < self.activity_value(activity_field::RUSH_DEADLINE)?
        {
            self.add_at(a + activity_field::RUSH_PROGRESS, 1, "rush_click")?;
            if self.activity_value(activity_field::RUSH_PROGRESS)?
                >= self.activity_value(activity_field::RUSH_TARGET)?
            {
                self.store(a + activity_field::RUSH_STATE, 2, "finish_baking_rush")?;
            }
        }
        Ok(())
    }
    pub(crate) fn advance_activity_tick(&mut self) -> Result<()> {
        let state = self.activity_value(activity_field::RUSH_STATE)?;
        if (state == 0 || state == 3)
            && self.tick >= self.activity_value(activity_field::RUSH_NEXT)?
        {
            self.offer_rush()?;
        } else if (state == 1 || state == 4)
            && self.tick >= self.activity_value(activity_field::RUSH_DEADLINE)?
        {
            self.store(
                self.activities_address()? + activity_field::RUSH_STATE,
                if state == 1 { 3 } else { 0 },
                "rush_time_up",
            )?;
            self.schedule_rush()?;
        }
        Ok(())
    }
    fn schedule_rush(&mut self) -> Result<()> {
        let next = self.next_event_tick(90, 180);
        self.store(
            self.activities_address()? + activity_field::RUSH_NEXT,
            next,
            "schedule_rush",
        )
    }
    pub fn claim_rush(&mut self) -> Result<()> {
        let a = self.activities_address()?;
        if self.activity_value(activity_field::RUSH_STATE)? != 2 {
            return Err(GameError::Locked);
        }
        self.award(
            self.activity_value(activity_field::RUSH_REWARD)?,
            "baking_rush_reward",
        )?;
        self.add_at(a + activity_field::RUSH_WINS, 1, "rush_win")?;
        self.store(a + activity_field::RUSH_STATE, 0, "claim_baking_rush")?;
        self.schedule_rush()
    }
    pub fn shower_reward(&self) -> Result<u64> {
        Ok(self
            .value(field::CPS)?
            .saturating_mul(self.recipe_production_multiplier()?)
            .saturating_mul(60)
            .saturating_add(self.click_power()?.saturating_mul(25))
            .max(100))
    }
    pub fn claim_cookie_shower(&mut self) -> Result<()> {
        let e = self.object(field::EFFECTS)?;
        if self.tick < self.memory.read_u64(e + 16)? {
            return Err(GameError::Locked);
        }
        let reward = self.shower_reward()?;
        self.award(reward, "cookie_shower")?;
        self.store(
            self.activities_address()? + activity_field::LAST_SHOWER,
            reward,
            "cookie_shower_reward",
        )?;
        self.schedule_gift()
    }
    pub(crate) fn reset_activities_after_prestige(&mut self) -> Result<()> {
        let a = self.activities_address()?;
        for offset in [
            activity_field::RUSH_STATE,
            activity_field::RUSH_PROGRESS,
            activity_field::RUSH_DEADLINE,
            activity_field::RUSH_NEXT,
        ] {
            self.store(a + offset, 0, "prestige_activities")?;
        }
        for i in 0..ORDER_COUNT {
            self.refresh_order(i)?;
        }
        self.schedule_rush()?;
        self.dismiss_enemy()
    }
    pub fn activities_view(&self) -> Result<ActivitiesView> {
        let value = |offset| self.activity_value(offset);
        let recipe = value(activity_field::RECIPE)?;
        if recipe >= RECIPES.len() as u64 {
            return Err(GameError::InvalidState("unknown recipe".into()));
        }
        let rush_state = match value(activity_field::RUSH_STATE)? {
            0 => RushState::Idle,
            1 => RushState::Running,
            2 => RushState::Ready,
            3 => RushState::Missed,
            4 => RushState::Offered,
            _ => return Err(GameError::InvalidState("unknown baking rush state".into())),
        };
        let mut recipes_unlocked = [false; 3];
        for (i, unlocked) in recipes_unlocked.iter_mut().enumerate() {
            *unlocked = value(activity_field::RECIPE_FLAGS + i as u64 * 8)? != 0;
        }
        Ok(ActivitiesView {
            recipe: recipe as usize,
            recipes_unlocked,
            orders: (0..ORDER_COUNT)
                .map(|i| self.order_view(i))
                .collect::<Result<_>>()?,
            orders_done: value(activity_field::ORDERS_DONE)?,
            rush_wins: value(activity_field::RUSH_WINS)?,
            rush_state,
            rush_target: value(activity_field::RUSH_TARGET)?,
            rush_progress: value(activity_field::RUSH_PROGRESS)?,
            rush_reward: value(activity_field::RUSH_REWARD)?,
            rush_remaining: value(activity_field::RUSH_DEADLINE)?.saturating_sub(self.tick),
            rush_cooldown: value(activity_field::RUSH_NEXT)?.saturating_sub(self.tick),
            shower_reward: self.shower_reward()?,
        })
    }
}
