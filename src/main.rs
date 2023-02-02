use std::collections::HashMap;
use std::thread::sleep;
use std::time::Duration;
use std::cmp::Ordering;
use std::fmt;
use std::fmt::Debug;
use uuid::Uuid;
use plotters::prelude::*;

type GoodUid = usize;
type Price = f64;

const GOODS: [&str; 1] = ["Grain"];

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

#[derive(Debug)]
struct StaticConsumptionTarget {
    good_uid: GoodUid,
    quantity: u64,
    target_quantity: u64,
    money_balance: f64,
    prestige: f64,
    consumption_rate: u64,
    orders_uuid: Vec<Uuid>,
}

impl fmt::Display for StaticConsumptionTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StaticConsumptionTarget")
            .field("quantity", &self.quantity)
            .field("money_balance", &self.money_balance)
            .field("consumption_rate", &self.consumption_rate)
            .finish()
    }
}

impl EcoEntity for StaticConsumptionTarget {
    fn produce_and_consume(&mut self) -> f64 {
        self.quantity -= self.consumption_rate;
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
        if self.quantity > self.target_quantity {
            return;
        }
        let required = self.target_quantity - self.quantity;
        let market = markets.first_mut().unwrap();
        let uuid = market.register_order(OrderType::Buy, required, self.prestige);
        self.orders_uuid.push(uuid);
    }

    fn retrieve_orders_from_markets(&mut self, markets: &mut [Box<dyn Market>]) {
        for uuid in self.orders_uuid.iter() {
            let result = markets.first_mut().unwrap().retrieve_order_result(uuid).unwrap();
            match result.ordertype {
                OrderType::Buy => {
                    self.quantity += result.traded_quantity;
                    self.money_balance -= result.total_cost;
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

#[derive(Debug)]
struct StaticProductionTarget {
    good_uid: GoodUid,
    quantity: u64,
    target_quantity: u64,
    money_balance: f64,
    prestige: f64,
    production_rate: u64,
    orders_uuid: Vec<Uuid>,
}

impl fmt::Display for StaticProductionTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StaticProductionTarget")
            .field("quantity", &self.quantity)
            .field("money_balance", &self.money_balance)
            .field("production_rate", &self.production_rate)
            .finish()
    }
}

impl EcoEntity for StaticProductionTarget {
    fn produce_and_consume(&mut self) -> f64 {
        self.quantity += self.production_rate;
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
    let mut producer = StaticProductionTarget {
        good_uid: 0,
        quantity: 12000,
        target_quantity: 10000,
        money_balance: 500_000.0,
        prestige: 5.0,
        production_rate: 500,
        orders_uuid: vec![],
    };
    let mut consumer = StaticConsumptionTarget {
        good_uid: 0,
        quantity: 5000,
        target_quantity: 10000,
        money_balance: 0.0,
        prestige: 5.0,
        consumption_rate: 400,
        orders_uuid: vec![],
    };
    let mut markets: Vec<Box<dyn Market>> = vec![
        Box::new(TestMarket {
            good_uid: 0,
            price_per_unit: 10.0,
            buy_orders: vec![],
            sell_orders: vec![],
        })];
    println!("producer debug: {:?}", &producer);
    println!("consumer debug: {:?}", &consumer);
    println!("market debug: {:?}", &markets[0]);
    // Data to plot
    let mut prod_inv = Vec::<u64>::new();
    let mut cons_inv = Vec::<u64>::new();
    for _ in 0..20 {
        println!("{}\n{}\nmarket price: {}", &producer, &consumer, &markets[0].price_per_unit());
        // Register
        prod_inv.push(producer.quantity);
        cons_inv.push(consumer.quantity);
        // Sleep
        //sleep(Duration::from_millis(500));
        // Step 1 - Resolve production and consumption of Economic Entities
        producer.produce_and_consume();
        consumer.produce_and_consume();
        // Step 2 - Get requested goods and custom zone metadata to choose what market expose to entities
        //   For now we ignore this but still call the function.
        producer.get_required_markets();
        consumer.get_required_markets();
        // Step 3 - Tell the entities to register their orders to the markets
        producer.post_orders_to_markets(&mut markets[..]);
        consumer.post_orders_to_markets(&mut markets[..]);
        // Step 4 - Run the trade algo in the markets
        for market in markets.iter_mut() {
            let traded = market.run_trade().unwrap();
            println!("traded: {traded}");
        }
        // Step 5 - Tell the entities to retrieve the results of the trade
        producer.retrieve_orders_from_markets(&mut markets[..]);
        consumer.retrieve_orders_from_markets(&mut markets[..]);
        // Step 6 - Clear the market internal status
        for market in markets.iter_mut() {
            market.clear_state();
        }
    }
    // Plot
    let root = BitMapBackend::new("out.png", (800, 600)).into_drawing_area();
    root.fill(&WHITE)?;
    let mut chart = ChartBuilder::on(&root)
        .margin(5)
        .caption("Quantity in inventory", ("sans-serif", 40).into_font())
        .set_left_and_bottom_label_area_size(40)
        .build_cartesian_2d(-0f32..20f32, -0f32..20000f32)?;
    chart.configure_mesh().draw()?;
    chart
        .draw_series(LineSeries::new(
            (0..20).zip(prod_inv.iter()).map(|(x,y)| (x as f32, *y as f32)),
            &RED,
        ))?
        .label("Producer")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], RED));
    chart
        .draw_series(LineSeries::new(
            (0..20).zip(cons_inv.iter()).map(|(x,y)| (x as f32, *y as f32)),
            &GREEN,
        ))?
        .label("Consumer")
        .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], GREEN));
    chart.configure_series_labels().position(SeriesLabelPosition::UpperRight).border_style(BLACK).draw()?;
    root.present()?;
    Ok(())
}
