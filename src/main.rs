use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration;
use std::cmp::Ordering;
use std::fmt;
use std::fmt::Debug;
use uuid::Uuid;
use plotters::prelude::*;
use plotters::style::full_palette::PURPLE;

type GoodUid = usize;
type Price = f64;

const GOODS: [&str; 2] = ["Grain", "Groceries"];

fn get_good_name(gooduid: GoodUid) -> String {
    GOODS[gooduid].to_owned()
}

type MarketMetadata = String;

enum OrderType {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
struct OrderInfo {
    uuid: Uuid,
    required_quantity: u64,
    traded_quantity: u64,
    prestige: f64,
}

impl OrderInfo {
    fn new(uuid: Uuid, required_quantity: u64, prestige: f64) -> OrderInfo {
        OrderInfo { uuid, required_quantity, prestige, traded_quantity: 0 }
    }

    fn missing_quantity(&self) -> u64 {
        self.required_quantity - self.traded_quantity
    }
}

struct OrderResult {
    ordertype: OrderType,
    traded_quantity: u64,
    total_cost: Price,
}

impl OrderResult {
    fn new(ordertype: OrderType, traded_quantity: u64, total_cost: Price) -> OrderResult {
        OrderResult { ordertype, traded_quantity, total_cost }
    }
}

trait Market: Debug {
    fn good_uid(&self) -> GoodUid;
    fn price_per_unit(&self) -> Price;
    // called from Step 2 in EcoEntity
    fn register_order(&mut self, otype: OrderType, quantity: u64, prestige: f64) -> Uuid;
    // Step 3
    fn run_trade(&mut self) -> Result<u64, ()>;
    fn retrieve_order_result(&mut self, uuid: &Uuid) -> Option<OrderResult>;
    // Step 6
    fn clear_state(&mut self);
}
// quando market registra un order ritorna un uuid che va segnato e usato per il recovery del result

trait EcoEntity {
    // Step 1
    fn produce_and_consume(&mut self) -> f64;
    // Step 2
    fn get_required_markets(&self) -> (Vec<GoodUid>, Vec<MarketMetadata>);
    // Step 4
    fn post_orders_to_markets(&mut self, markets: &mut [Box<dyn Market>]);
    // Step 5
    fn retrieve_orders_from_markets(&mut self, markets: &mut [Box<dyn Market>]);
}

struct RGOSingle {
    good_uid: GoodUid,
    // Inventory
    quantity: u64,
    // Inventory desired quantity
    target_quantity: u64,
    // Production
    max_production_rate: u64,
    // Costs
    per_unit_cost: f64,
    fixed_cost: f64,
    // Others
    money_balance: f64,
    prestige: f64,
    orders_uuid: Vec<Uuid>,
}

impl EcoEntity for RGOSingle {
    fn produce_and_consume(&mut self) -> f64 {
        let enough_money_to_output = ((self.money_balance - self.fixed_cost) / self.per_unit_cost) as u64;
        let output_value = self.max_production_rate.min(enough_money_to_output);
        self.quantity += output_value;
        self.money_balance -= output_value as f64 * self.per_unit_cost + self.fixed_cost;
        0.
    }

    fn get_required_markets(&self) -> (Vec<GoodUid>, Vec<MarketMetadata>) {
        let goods = vec![self.good_uid];
        let metadata = vec![
            "ita".to_owned()
        ];
        (goods, metadata)
    }

    fn post_orders_to_markets(&mut self, markets: &mut [Box<dyn Market>]) {
        if self.quantity < self.target_quantity {
            return;
        }
        let required = self.quantity - self.target_quantity;
        let market = markets.first_mut().unwrap();
        let uuid = market.register_order(OrderType::Sell, required, self.prestige);
        self.orders_uuid.push(uuid);
    }

    fn retrieve_orders_from_markets(&mut self, markets: &mut [Box<dyn Market>]) {
        for uuid in self.orders_uuid.iter() {
            let result = markets.first_mut().unwrap().retrieve_order_result(uuid).unwrap();
            match result.ordertype {
                OrderType::Buy => {
                    self.quantity += result.traded_quantity;
                    self.money_balance -= result.total_cost;
                    unreachable!()
                }
                OrderType::Sell => {
                    self.quantity -= result.traded_quantity;
                    self.money_balance += result.total_cost;
                }
            }
        }
        self.orders_uuid.clear();
    }
}

struct BasicPop {
    // The pop require full goods input and ask them with a priority order
    // Invetory
    goods_inventory: HashMap<GoodUid, u64>,
    // Inventory desired quantity
    goods_priority_order: Vec<GoodUid>,
    goods_desired_inventory: HashMap<GoodUid, u64>,
    // Consumption
    consumed_goods_per_tick: HashMap<GoodUid, u64>,
    // Others
    money_balance: f64,
    money_increase_per_tick: f64,
    prestige: f64,
    standard_of_living: f64,
    goods_buy_orders_uuid: HashMap<GoodUid, Vec<Uuid>>,
}

impl BasicPop {
    #[allow(clippy::too_many_arguments)]
    fn new(
        goods_in_prio_order: Vec<GoodUid>,
        inventory_goods_in_order: Vec<u64>,
        desired_inv_goods_in_order: Vec<u64>,
        consumed_goods_in_order: Vec<u64>,
        money_balance: f64,
        money_increase_per_tick: f64,
        prestige: f64,
        standard_of_living: f64,
    ) -> BasicPop {
        assert_eq!(goods_in_prio_order.len(), consumed_goods_in_order.len());
        let goods_inventory = HashMap::from_iter(goods_in_prio_order.clone().into_iter().zip(inventory_goods_in_order.into_iter()));
        let goods_desired_inventory = HashMap::from_iter(goods_in_prio_order.clone().into_iter().zip(desired_inv_goods_in_order.into_iter()));
        let consumed_goods_per_tick = HashMap::from_iter(goods_in_prio_order.clone().into_iter().zip(consumed_goods_in_order.into_iter()));
        BasicPop {
            goods_inventory,
            goods_priority_order: goods_in_prio_order,
            goods_desired_inventory,
            consumed_goods_per_tick,
            money_balance,
            money_increase_per_tick,
            prestige,
            standard_of_living,
            goods_buy_orders_uuid: Default::default(),
        }
    }
}

impl EcoEntity for BasicPop {
    fn produce_and_consume(&mut self) -> f64 {
        let mut delta_sol = 0.;
        for good in self.goods_priority_order.iter() {
            let inventory = self.goods_inventory.get_mut(good).unwrap();
            let consumed_per_tick = self.consumed_goods_per_tick.get(good).unwrap();
            if *inventory >= *consumed_per_tick {
                *inventory -= consumed_per_tick;
                delta_sol += 1.;
            } else {
                let fract_missing = (*consumed_per_tick - *inventory) as f64 / (*consumed_per_tick as f64);
                delta_sol -= fract_missing;
            }
        }
        self.standard_of_living += delta_sol;
        delta_sol
    }

    fn get_required_markets(&self) -> (Vec<GoodUid>, Vec<MarketMetadata>) {
        let metadata = vec![
            "ita".to_owned()
        ];
        (self.goods_priority_order.clone(), metadata)
    }

    fn post_orders_to_markets(&mut self, markets: &mut [Box<dyn Market>]) {
        let mut actual_expense = 0.;
        for good in self.goods_priority_order.iter() {
            let market = markets.iter_mut().find(|x| x.good_uid() == *good).unwrap();
            let target_quantity = *self.goods_desired_inventory.get(good).unwrap();
            if self.goods_inventory[good] >= target_quantity {
                continue;
            }
            let aval_money = self.money_balance - actual_expense;
            let enough_money_to_buy = (aval_money / market.price_per_unit()) as u64;
            let required = (target_quantity - self.goods_inventory[good]).min(enough_money_to_buy);
            actual_expense += required as f64 * market.price_per_unit();
            let uuid = market.register_order(OrderType::Buy, required, self.prestige);
            self.goods_buy_orders_uuid.entry(*good).and_modify(|v| v.push(uuid)).or_insert(Vec::new());
        }
    }

    fn retrieve_orders_from_markets(&mut self, markets: &mut [Box<dyn Market>]) {
        for (good_uid, uuids) in self.goods_buy_orders_uuid.iter() {
            let market = markets.iter_mut().find(|x| x.good_uid() == *good_uid).unwrap();
            for uuid in uuids.iter() {
                let result = market.retrieve_order_result(uuid).unwrap();
                match result.ordertype {
                    OrderType::Buy => {
                        *self.goods_inventory.get_mut(good_uid).unwrap() += result.traded_quantity;
                        self.money_balance -= result.total_cost;
                    }
                    OrderType::Sell => {
                        *self.goods_inventory.get_mut(good_uid).unwrap() -= result.traded_quantity;
                        self.money_balance += result.total_cost;
                        unreachable!()
                    }
                }
            }
        }
        self.goods_buy_orders_uuid.clear();
    }
}

struct ProductorOneToOne {
    input_good_uid: GoodUid,
    output_good_uid: GoodUid,
    // Inventory
    input_quantity: u64,
    output_quantity: u64,
    // Inventory desired quantity
    target_input_quantity: u64,
    target_output_quantity: u64,
    // Conversions
    conversion_rateo: f64,
    target_input_per_tick: u64,
    // Operation costs TODO: use better parameters
    per_input_unit_cost: f64,
    fixed_cost: f64,
    // Others
    money_balance: f64,
    prestige: f64,
    input_orders_uuid: Vec<Uuid>,
    output_orders_uuid: Vec<Uuid>,
}

// TODO: Gestire il capital come capital_unit che e' equivalente al livello
//    del building e ad ogni livello aumenta il costo fisso dell'impresa
//        capital_unit_cost: f64,
//        input_per_capital_unit: f64

impl ProductorOneToOne {
    #[allow(dead_code, unused_variables)]
    fn production_cost_per_total_input(&self, total_input: u64) -> f64 {
        // TODO: l'idea e' usare questa funzione per calcolare salari e costo macchine di produzione
        //   l'idea alla base di questa funzione e' che il costo totale di produzione deve essere
        //   un unione dei costi fissi + costi variabili per elemento in modo analogo a come ho
        //   imparato nel libro magico di Economia
        // total_input as f64 * self.per_unit_fixed_cost;
        todo!()
    }
}

impl EcoEntity for ProductorOneToOne {
    fn produce_and_consume(&mut self) -> f64 {
        let enough_money_to_input = ((self.money_balance - self.fixed_cost) / self.per_input_unit_cost) as u64;
        let input_value = self.input_quantity.min(self.target_input_per_tick).min(enough_money_to_input);
        let output_value = (input_value as f64 * self.conversion_rateo) as u64;
        self.input_quantity -= input_value;
        self.output_quantity += output_value;
        self.money_balance -= input_value as f64 * self.per_input_unit_cost + self.fixed_cost;
        0.
    }

    fn get_required_markets(&self) -> (Vec<GoodUid>, Vec<MarketMetadata>) {
        let goods = vec![self.input_good_uid, self.output_good_uid];
        let metadata = vec![
            "ita".to_owned()
        ];
        (goods, metadata)
    }

    fn post_orders_to_markets(&mut self, markets: &mut [Box<dyn Market>]) {
        // Individuate input and output markets
        // see https://stackoverflow.com/questions/30073684/how-to-get-mutable-references-to-two-array-elements-at-the-same-time
        // for why we need to allow us to take two mutable from the slice
        // we need to take them separately in separate scopes so that the &mut on
        // markets get free again after you finished the use of input_market
        {
            let input_market = markets.iter_mut().find(|x| x.good_uid() == self.input_good_uid)
                .expect("No input market for the requested good");
            // Check if more input is needed
            if self.input_quantity < self.target_input_quantity {
                let mut required = self.target_input_quantity - self.input_quantity;
                if required as f64 * input_market.price_per_unit() > self.money_balance {
                    required = (self.money_balance / input_market.price_per_unit()) as u64;
                }
                let uuid = input_market.register_order(OrderType::Buy, required, self.prestige);
                self.input_orders_uuid.push(uuid);
            }
        }
        {
            let output_market = markets.iter_mut().find(|x| x.good_uid() == self.output_good_uid)
                .expect("No output market for the producer good");
            // Check if you have output to sell
            if self.output_quantity > self.target_output_quantity {
                let required = self.output_quantity - self.target_output_quantity;
                let uuid = output_market.register_order(OrderType::Sell, required, self.prestige);
                self.output_orders_uuid.push(uuid);
            }
        }
    }

    fn retrieve_orders_from_markets(&mut self, markets: &mut [Box<dyn Market>]) {
        {
            let input_market = markets.iter_mut().find(|x| x.good_uid() == self.input_good_uid)
                .expect("No input market for the requested good");
            for uuid in self.input_orders_uuid.iter() {
                let result = input_market.retrieve_order_result(uuid).unwrap();
                assert!(matches!(result.ordertype, OrderType::Buy));
                self.input_quantity += result.traded_quantity;
                self.money_balance -= result.total_cost;
            }
            self.input_orders_uuid.clear();
        }
        {
            let output_market = markets.iter_mut().find(|x| x.good_uid() == self.output_good_uid)
                .expect("No output market for the producer good");
            for uuid in self.output_orders_uuid.iter() {
                let result = output_market.retrieve_order_result(uuid).unwrap();
                assert!(matches!(result.ordertype, OrderType::Sell));
                self.output_quantity -= result.traded_quantity;
                self.money_balance += result.total_cost;
            }
            self.output_orders_uuid.clear();
        }
    }
}

#[derive(Debug)]
struct TestMarket {
    good_uid: GoodUid,
    price_per_unit: Price,
    buy_orders: Vec<OrderInfo>,
    sell_orders: Vec<OrderInfo>,
}

impl TestMarket {
    fn distribute(&self, total_to_dist: u64, recvarray: &mut [OrderInfo]) -> u64 {
        let mut dist_for_now = 0_u64;
        loop {
            let not_fulled = recvarray.iter().filter(|x| x.traded_quantity != x.required_quantity).count();
            if not_fulled == 0 { break; }
            let eq_chunks = (total_to_dist - dist_for_now) / not_fulled as u64;
            if eq_chunks == 0 { break; }
            let distributed = recvarray.iter_mut().filter(|x| x.traded_quantity != x.required_quantity)
                .fold(0_u64, |distributed, x| {
                    x.traded_quantity += eq_chunks;
                    if x.traded_quantity > x.required_quantity {
                        let rem = x.traded_quantity - x.required_quantity;
                        x.traded_quantity -= rem;
                        return distributed + eq_chunks - rem;
                    }
                    distributed + eq_chunks
                });
            dist_for_now += distributed;
            if distributed == 0 { break; }
        }
        // Distribute the remainder
        let mut remainder = total_to_dist - dist_for_now;
        for bo in recvarray.iter_mut().filter(|x| x.traded_quantity != x.required_quantity) {
            if remainder > 0 {
                bo.traded_quantity += 1;
                dist_for_now += 1;
                remainder -= 1;
            } else {
                break;
            }
        }
        // Return the distributed quantity
        dist_for_now
    }

    fn trade_loop(
        &self,
        distrarray: &mut [OrderInfo],
        recvarray: &mut [OrderInfo],
        total_to_dist: u64,
    ) -> u64 {
        // This function thinks that recvarray has more receiving quantity than the one that is been distributing.
        // This is how to obtain here the value. Unnecessary heavy task that I already do one time outside the fn
        // let total_dist = distrarray.iter().fold(0, |acc, x| acc + x.required_quantity - x.traded_quantity);
        // Distribute the trade value equally between all the orders not full
        let distributed = self.distribute(total_to_dist, recvarray);
        // Report the distribution to the distributors
        // We have to run the distribution algo for the distributors too to see who selled what
        let chk_dist = self.distribute(distributed, distrarray);
        assert_eq!(distributed, chk_dist);
        // Return the total distributed
        distributed
    }
}

impl Market for TestMarket {
    fn good_uid(&self) -> GoodUid {
        self.good_uid
    }

    fn price_per_unit(&self) -> Price {
        self.price_per_unit
    }

    fn register_order(&mut self, otype: OrderType, quantity: u64, prestige: f64) -> Uuid {
        let uuid = Uuid::new_v4();
        match otype {
            OrderType::Buy => {
                self.buy_orders.push(OrderInfo::new(uuid, quantity, prestige))
            }
            OrderType::Sell => {
                self.sell_orders.push(OrderInfo::new(uuid, quantity, prestige))
            }
        }
        // println!("register_order: {:?} {:?} - {uuid}", &self.buy_orders, &self.sell_orders);
        uuid
    }

    fn run_trade(&mut self) -> Result<u64, ()> {
        // TODO: calculate price delta
        if self.buy_orders.is_empty() || self.sell_orders.is_empty() {
            return Ok(0);
        }
        let mut total_final_traded: u64 = 0;
        let mut buymap = HashMap::<i64, Vec<OrderInfo>>::new();
        for bo in self.buy_orders.iter() {
            buymap.entry(bo.prestige as i64).and_modify(|v| v.push(bo.clone())).or_insert(vec![bo.clone()]);
        }
        let mut sellmap = HashMap::<i64, Vec<OrderInfo>>::new();
        for bo in self.sell_orders.iter() {
            sellmap.entry(bo.prestige as i64).and_modify(|v| v.push(bo.clone())).or_insert(vec![bo.clone()]);
        }
        let mut buyvaliter = buymap.into_values();
        let mut sellvaliter = sellmap.into_values();

        let mut buyarray = buyvaliter.next().unwrap();
        let mut sellarray = sellvaliter.next().unwrap();

        let mut result_buyarray = Vec::<OrderInfo>::new();
        let mut result_sellarray = Vec::<OrderInfo>::new();
        'main: loop {
            let total_buy = buyarray.iter().fold(0, |acc, x| acc + x.required_quantity - x.traded_quantity);
            let total_sell = sellarray.iter().fold(0, |acc, x| acc + x.required_quantity - x.traded_quantity);
            match total_sell.cmp(&total_buy) {
                Ordering::Greater => {
                    // TS > TB => Distribute the product from the buyers to the sellers that are more of them so
                    //   it's guaranteed that all the buyers will finish with full trade!
                    let total_traded = self.trade_loop(
                        &mut buyarray[..],
                        &mut sellarray[..],
                        total_buy,
                    );
                    assert_eq!(total_traded, total_buy);
                    total_final_traded += total_traded;
                    // The buyer selected have finished what they had to distribute. Take next
                    //  and register the finished orders in the result
                    result_buyarray.append(&mut buyarray);
                    if let Some(x) = buyvaliter.next() {
                        // There is another
                        buyarray = x;
                    } else {
                        // We finished the new buyers! Exit.
                        result_sellarray.append(&mut sellarray);
                        break 'main;
                    }
                }
                Ordering::Less => {
                    // TS < TB => Distribute the product from the sellers to the buyers that are more of them so
                    //   it's guaranteed that all the sellers will finish with full trade!
                    let total_traded = self.trade_loop(
                        &mut sellarray[..],
                        &mut buyarray[..],
                        total_sell,
                    );
                    assert_eq!(total_traded, total_sell);
                    total_final_traded += total_traded;
                    // The sellers selected have finished what they had to distribute. Take next
                    //  and register the finished orders in the result
                    result_sellarray.append(&mut sellarray);
                    if let Some(x) = sellvaliter.next() {
                        // There is another
                        sellarray = x;
                    } else {
                        // We finished the new sellers! Exit.
                        result_buyarray.append(&mut buyarray);
                        break 'main;
                    }
                }
                Ordering::Equal => {
                    // TS == TB => this batch of sellers and buyers have the exact same quantity!
                    for bo in buyarray.iter_mut() {
                        bo.traded_quantity = bo.required_quantity;
                    }
                    for bo in sellarray.iter_mut() {
                        bo.traded_quantity = bo.required_quantity;
                    }
                    total_final_traded += total_buy;  // Same as total_sell
                    // Save the results
                    result_buyarray.append(&mut buyarray);
                    result_sellarray.append(&mut sellarray);
                    // The buyer selected have finished what they had to distribute. Take next
                    if let Some(x) = buyvaliter.next() {
                        // There is another
                        buyarray = x;
                    } else {
                        // We finished the new buyers! Exit.
                        break 'main;
                    }
                    // The buyer selected have finished what they had to distribute. Take next
                    if let Some(x) = sellvaliter.next() {
                        // There is another
                        sellarray = x;
                    } else {
                        // We finished the new buyers! Exit.
                        break 'main;
                    }
                }
            }
        }
        self.buy_orders = result_buyarray;
        self.sell_orders = result_sellarray;
        Ok(total_final_traded)
    }

    fn retrieve_order_result(&mut self, uuid: &Uuid) -> Option<OrderResult> {
        if let Some(x) = self.buy_orders.iter().find(|x| &x.uuid == uuid) {
            Some(OrderResult::new(
                OrderType::Buy,
                x.traded_quantity,
                x.traded_quantity as f64 * self.price_per_unit))
        } else if let Some(x) = self.sell_orders.iter().find(|x| &x.uuid == uuid) {
            Some(OrderResult::new(
                OrderType::Sell,
                x.traded_quantity,
                x.traded_quantity as f64 * self.price_per_unit))
        } else {
            None
        }
    }

    fn clear_state(&mut self) {
        self.buy_orders.clear();
        self.sell_orders.clear();
        // TODO: are we sure they are empty/all the results has been retrieved?
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut rgo = RGOSingle {
        good_uid: 0,
        quantity: 1000,
        target_quantity: 1000,
        max_production_rate: 500,
        per_unit_cost: 1.0,
        fixed_cost: 500.0,
        money_balance: 10_000.0,
        prestige: 0.0,
        orders_uuid: vec![],
    };
    // Min Sell Price of 0 now is 2.0$ per unit (500 unit costs 1000$)
    // TODO: implement RGO that allow to "lose a percentage" on unselled goods
    let mut factory = ProductorOneToOne {
        input_good_uid: 0,
        output_good_uid: 1,
        input_quantity: 600,
        output_quantity: 600,
        target_input_quantity: 900,
        target_output_quantity: 900,
        conversion_rateo: 0.5,
        target_input_per_tick: 300,
        per_input_unit_cost: 1.0,
        fixed_cost: 500.0,
        money_balance: 10_000.0,
        prestige: 0.0,
        input_orders_uuid: vec![],
        output_orders_uuid: vec![],
    };
    // Buying at min price 2.0$pu you spend:
    // 600$ per 300 input
    // pay 500$ per fixed cost
    // pay 1$pu as var cost = 300$
    // total 600$ + 800$ = 1400$ per 150 output
    // Min price for good2 = 1400/150 = 9.34$pu
    let mut pop = BasicPop::new(
        vec![0, 1],
        vec![600, 450],
        vec![400, 300],
        vec![200, 150],
        6_000.0,
        2_000.0,
        -1.0,
        0.0,
    );
    // Accounting the residue prod of RGO is 200 g0 and the output of factory
    // is 150 g1, the pop will require every cycle that.
    // The min price of all that is
    // g0 = 200*2$ = 400$
    // g1 = 150*9.34$ approx 150*10$ = 1500$
    // Total min month price = 1900$
    // CreateMarkets
    let mut markets: Vec<Box<dyn Market>> = vec![
        Box::new(TestMarket {
            good_uid: 0,
            price_per_unit: 2.0,
            buy_orders: vec![],
            sell_orders: vec![],
        }),
        Box::new(TestMarket {
            good_uid: 1,
            price_per_unit: 10.0,
            buy_orders: vec![],
            sell_orders: vec![],
        }),
    ];
    // Data for the plots
    let mut rgo_money = Vec::<f64>::new();
    let mut factory_money = Vec::<f64>::new();
    let mut pop_money = Vec::<f64>::new();
    let mut rgo_g0 = Vec::<u64>::new();
    let mut factory_g0 = Vec::<u64>::new();
    let mut factory_g1 = Vec::<u64>::new();
    let mut pop_g0 = Vec::<u64>::new();
    let mut pop_g1 = Vec::<u64>::new();
    for _ in 0..20 {
        // Register
        rgo_money.push(rgo.money_balance);
        factory_money.push(factory.money_balance);
        pop_money.push(pop.money_balance);
        rgo_g0.push(rgo.quantity);
        factory_g0.push(factory.input_quantity);
        factory_g1.push(factory.output_quantity);
        pop_g0.push(pop.goods_inventory[&0]);
        pop_g1.push(pop.goods_inventory[&1]);
        // Sleep
        // sleep(Duration::from_millis(500));
        // Step 1 - Resolve production and consumption of Economic Entities
        rgo.produce_and_consume();
        factory.produce_and_consume();
        pop.produce_and_consume();
        // Step 2 - Get requested goods and custom zone metadata to choose what market expose to entities
        //   For now we ignore this but still call the function.
        rgo.get_required_markets();
        factory.get_required_markets();
        pop.get_required_markets();
        // Step 3 - Tell the entities to register their orders to the markets
        rgo.post_orders_to_markets(&mut markets[..1]);
        factory.post_orders_to_markets(&mut markets[..]);
        pop.post_orders_to_markets(&mut markets[..]);
        // Step 4 - Run the trade algo in the markets
        for market in markets.iter_mut() {
            let traded = market.run_trade().unwrap();
            println!("traded: {traded}");
        }
        // Step 5 - Tell the entities to retrieve the results of the trade
        rgo.retrieve_orders_from_markets(&mut markets[..1]);
        factory.retrieve_orders_from_markets(&mut markets[..]);
        pop.retrieve_orders_from_markets(&mut markets[..]);
        // Step 6 - Clear the market internal status
        for market in markets.iter_mut() {
            market.clear_state();
        }
    }
    // Plots
    // Money Plot
    let root = BitMapBackend::new("out_money.png", (800, 600)).into_drawing_area();
    root.fill(&WHITE)?;
    let max = rgo_money.iter().max_by(|a, b| a.total_cmp(b)).unwrap()
        .max(*factory_money.iter().max_by(|a, b| a.total_cmp(b)).unwrap())
        .max(*pop_money.iter().max_by(|a, b| a.total_cmp(b)).unwrap());
    let mut chart = ChartBuilder::on(&root)
        .margin(5)
        .caption("Money Balance", ("sans-serif", 20).into_font())
        .set_left_and_bottom_label_area_size(40)
        .build_cartesian_2d(0.0_f64..20.0, 0.0_f64..max)?;
    chart.configure_mesh().draw()?;
    chart
        .draw_series(LineSeries::new(
            (0..20).map(|x| x as f64).zip(rgo_money),
            ShapeStyle::from(RED).stroke_width(2),
        ))?
        .label("RGO")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], RED));
    chart
        .draw_series(LineSeries::new(
            (0..20).map(|x| x as f64).zip(factory_money),
            ShapeStyle::from(YELLOW).stroke_width(2),
        ))?
        .label("Factory")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], YELLOW));
    chart
        .draw_series(LineSeries::new(
            (0..20).map(|x| x as f64).zip(pop_money),
            ShapeStyle::from(GREEN).stroke_width(2),
        ))?
        .label("Pop")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], GREEN));
    chart.configure_series_labels()
        .position(SeriesLabelPosition::LowerRight)
        .border_style(BLACK)
        .draw()?;
    root.present()?;
    // Goods inventory plot
    let root = BitMapBackend::new("out_inventory.png", (800, 600)).into_drawing_area();
    root.fill(&WHITE)?;
    let max = *rgo_g0.iter().max().unwrap()
        .max(factory_g0.iter().max().unwrap())
        .max(factory_g1.iter().max().unwrap())
        .max(pop_g0.iter().max().unwrap())
        .max(pop_g1.iter().max().unwrap());
    let mut chart = ChartBuilder::on(&root)
        .margin(5)
        .caption("Goods Inventory", ("sans-serif", 20).into_font())
        .set_left_and_bottom_label_area_size(40)
        .build_cartesian_2d(0.0_f64..20.0, 0.0_f64..max as f64)?;
    chart.configure_mesh().draw()?;
    chart
        .draw_series(LineSeries::new(
            (0..20).map(|x| x as f64).zip(rgo_g0.into_iter().map(|x| x as f64)),
            ShapeStyle::from(RED).stroke_width(2),
        ))?
        .label("RGO Good 0")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], RED));
    chart
        .draw_series(LineSeries::new(
            (0..20).map(|x| x as f64).zip(factory_g0.into_iter().map(|x| x as f64)),
            ShapeStyle::from(YELLOW).stroke_width(2),
        ))?
        .label("Factory Good 0")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], YELLOW));
    chart
        .draw_series(LineSeries::new(
            (0..20).map(|x| x as f64).zip(factory_g1.into_iter().map(|x| x as f64)),
            ShapeStyle::from(BLUE).stroke_width(2),
        ))?
        .label("Factory Good 1")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], BLUE));
    chart
        .draw_series(LineSeries::new(
            (0..20).map(|x| x as f64).zip(pop_g0.into_iter().map(|x| x as f64)),
            ShapeStyle::from(PURPLE).stroke_width(2),
        ))?
        .label("Pop Good 0")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], PURPLE));
    chart
        .draw_series(LineSeries::new(
            (0..20).map(|x| x as f64).zip(pop_g1.into_iter().map(|x| x as f64)),
            ShapeStyle::from(GREEN).stroke_width(2),
        ))?
        .label("Pop Good 1")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], GREEN));
    chart.configure_series_labels()
        .position(SeriesLabelPosition::LowerRight)
        .border_style(BLACK)
        .draw()?;
    root.present()?;
    Ok(())
}
