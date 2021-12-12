# DT Bigquery

The purpose of this project is to persistently store events from [Disruptive Technologies](https://www.disruptive-technologies.com/) sensors into a [Google Bigquery](https://cloud.google.com/bigquery) table for later processing. Common fields are extracted from the event and added as seperate columns, while the `data` field is stored as a JSON string in order to support both current and future sensor events. 

The service is written in Rust and intended to be deployed as a Docker container. 