from lib_sql_fuzzer import parse_sql


def main():
    sql = parse_sql("SELECT A FROM B", "Sqlite")
    print(sql)

if __name__ == "__main__":
    main()
